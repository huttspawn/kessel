"""Diff the itemization of two default extracts (an old build and a new one) for a gear planner.

usage: python3 -I pts_item_diff.py <old spice.sqlite> <new spice.sqlite> > diff.json

Compares, table by table: item_rating_table, item_budget_table, item_modifier_packages, item_stats
(rating bands, stat names, per-item value changes), item_details (kinds, rarities, sources, slots),
item families (FQN prefixes added or removed), item_sets and item_set_members, item_sources,
stat_curve_values, relic_procs, and the singletons whose payload bytes changed. Item names join
through strings 'str.itm.0.<string_id>'. Both files are opened read-only.
"""
import collections
import json
import sqlite3
import sys


def open_ro(path):
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def rows(c, sql):
    return c.execute(sql).fetchall()


def counter(c, sql):
    return {str(k): v for k, v in rows(c, sql)}


def diff_counts(a, b):
    keys = sorted(set(a) | set(b))
    return {k: [a.get(k, 0), b.get(k, 0)] for k in keys if a.get(k, 0) != b.get(k, 0)}


def family(fqn, depth):
    return ".".join(fqn.split(".")[:depth])


def names(c):
    return dict(
        rows(
            c,
            "select o.fqn, s.text from objects o join strings s on s.fqn = 'str.itm.0.' || o.string_id "
            "where o.kind = 'Item' and o.is_canonical = 1",
        )
    )


def rating_bands(c):
    """Distinct items per display rating, and per item its rating and stat map."""
    per = collections.defaultdict(dict)
    rating = {}
    for fqn, r, stat, value in rows(c, "select item_fqn, rating, stat_name, value from item_stats"):
        per[fqn][stat] = value
        rating[fqn] = r
    return per, rating


def main():
    old_path, new_path = sys.argv[1:]
    a, b = open_ro(old_path), open_ro(new_path)
    out = {}

    tables = [
        "items", "item_details", "item_stats", "item_rating_table", "item_budget_table",
        "item_modifier_packages", "item_sets", "item_set_members", "item_sources",
        "item_granted_abilities", "item_unlocks", "stat_curve_values", "relic_procs", "singletons",
    ]
    out["row_counts"] = {t: [rows(a, f"select count(*) from {t}")[0][0], rows(b, f"select count(*) from {t}")[0][0]] for t in tables}

    # Rating table: rating per (level, quality).
    ra = {(l, q): r for l, q, r in rows(a, "select item_level, quality, rating from item_rating_table")}
    rb = {(l, q): r for l, q, r in rows(b, "select item_level, quality, rating from item_rating_table")}
    out["rating_table_changed"] = [
        {"level": l, "quality": q, "old": ra.get((l, q)), "new": rb.get((l, q))}
        for (l, q) in sorted(set(ra) | set(rb))
        if ra.get((l, q)) != rb.get((l, q))
    ]

    # Budget table: cells changed per quality and level.
    ba = {(q, l, p): v for q, l, p, v in rows(a, "select quality, item_level, permille, value from item_budget_table")}
    bb = {(q, l, p): v for q, l, p, v in rows(b, "select quality, item_level, permille, value from item_budget_table")}
    changed = collections.Counter()
    for k in set(ba) | set(bb):
        if ba.get(k) != bb.get(k):
            changed[(k[0], k[1])] += 1
    out["budget_cells_changed_by_quality_level"] = {f"{q}/{l}": n for (q, l), n in sorted(changed.items())}
    out["budget_levels"] = {
        "old": counter(a, "select quality, max(item_level) from item_budget_table group by 1"),
        "new": counter(b, "select quality, max(item_level) from item_budget_table group by 1"),
    }

    # Modifier packages.
    def packages(c):
        p = collections.defaultdict(dict)
        for m, s, pm in rows(c, "select mod_id, stat_name, permille from item_modifier_packages"):
            p[m][s] = pm
        return p

    pa, pb = packages(a), packages(b)
    out["modifier_packages"] = {
        "old": len(pa), "new": len(pb),
        "added": len(set(pb) - set(pa)), "removed": len(set(pa) - set(pb)),
        "changed": sum(1 for m in set(pa) & set(pb) if pa[m] != pb[m]),
        "added_samples": [{"mod_id": m, "split": pb[m]} for m in sorted(set(pb) - set(pa))[:10]],
        "changed_samples": [{"mod_id": m, "old": pa[m], "new": pb[m]} for m in sorted(set(pa) & set(pb)) if pa[m] != pb[m]][:10],
    }

    # Item stats and rating bands.
    sa, rta = rating_bands(a)
    sb, rtb = rating_bands(b)
    out["rating_histogram"] = {
        "old": dict(sorted(collections.Counter(rta.values()).items())),
        "new": dict(sorted(collections.Counter(rtb.values()).items())),
    }
    out["stat_names"] = diff_counts(
        counter(a, "select stat_name, count(*) from item_stats group by 1"),
        counter(b, "select stat_name, count(*) from item_stats group by 1"),
    )
    common = set(sa) & set(sb)
    out["items_with_stats"] = {
        "old": len(sa), "new": len(sb), "both": len(common),
        "only_old": len(set(sa) - set(sb)), "only_new": len(set(sb) - set(sa)),
        "stats_changed_on_shared": sum(1 for f in common if sa[f] != sb[f]),
        "rating_changed_on_shared": sum(1 for f in common if rta[f] != rtb[f]),
    }
    na, nb = names(a), names(b)
    top = sorted((f for f in sb if rtb[f] >= 345), key=lambda f: (-rtb[f], f))
    out["new_top_rating_items"] = {
        "count": len(top),
        "by_rating": dict(sorted(collections.Counter(rtb[f] for f in top).items())),
        "families": dict(collections.Counter(family(f, 4) for f in top).most_common(40)),
        "samples": [{"fqn": f, "name": nb.get(f), "rating": rtb[f], "stats": sb[f]} for f in top[:: max(1, len(top) // 40)]][:40],
    }
    out["stats_changed_samples"] = [
        {"fqn": f, "name": nb.get(f), "old": [rta[f], sa[f]], "new": [rtb[f], sb[f]]}
        for f in sorted(common) if sa[f] != sb[f]
    ][:25]

    # Item details.
    for col in ["item_kind", "rarity", "source", "slot"]:
        out[f"details_{col}"] = diff_counts(
            counter(a, f"select {col}, count(*) from item_details group by 1"),
            counter(b, f"select {col}, count(*) from item_details group by 1"),
        )
    da = {r[0]: r[1:] for r in rows(a, "select * from item_details")}
    db = {r[0]: r[1:] for r in rows(b, "select * from item_details")}
    out["items"] = {"old": len(da), "new": len(db), "added": len(set(db) - set(da)), "removed": len(set(da) - set(db)),
                    "details_changed": sum(1 for f in set(da) & set(db) if da[f] != db[f])}
    for depth in (3, 4):
        fa = collections.Counter(family(f, depth) for f in da)
        fb = collections.Counter(family(f, depth) for f in db)
        out[f"families_depth{depth}_added"] = {k: fb[k] for k in sorted(set(fb) - set(fa))}
        out[f"families_depth{depth}_removed"] = {k: fa[k] for k in sorted(set(fa) - set(fb))}
        out[f"families_depth{depth}_grew_or_shrank"] = {
            k: [fa[k], fb[k]] for k in sorted(set(fa) & set(fb)) if abs(fa[k] - fb[k]) >= 25
        }
    added = sorted(set(db) - set(da))
    out["added_item_samples"] = [{"fqn": f, "name": nb.get(f), "details": db[f]} for f in added[:: max(1, len(added) // 60)]][:60]
    out["added_by_kind"] = dict(collections.Counter(db[f][0] for f in added))
    removed = sorted(set(da) - set(db))
    out["removed_by_kind"] = dict(collections.Counter(da[f][0] for f in removed))
    out["removed_item_samples"] = [{"fqn": f, "name": na.get(f), "details": da[f]} for f in removed[:: max(1, len(removed) // 30)]][:30]

    # Augments and tacticals: list the new ones in full.
    for kind in ["augment", "augment_kit", "tactical", "setbonus"]:
        ka = {f for f, d in da.items() if d[0] == kind}
        kb = {f for f, d in db.items() if d[0] == kind}
        out[f"{kind}_added"] = [{"fqn": f, "name": nb.get(f), "rating": rtb.get(f), "stats": sb.get(f)} for f in sorted(kb - ka)][:80]
        out[f"{kind}_removed"] = [{"fqn": f, "name": na.get(f)} for f in sorted(ka - kb)][:80]

    # Sets.
    sta = {r[0]: r[1:] for r in rows(a, "select * from item_sets")}
    stb = {r[0]: r[1:] for r in rows(b, "select * from item_sets")}
    out["sets_added"] = sorted(set(stb) - set(sta))
    out["sets_removed"] = sorted(set(sta) - set(stb))
    ma = collections.Counter(s for (s,) in rows(a, "select set_fqn from item_set_members"))
    mb = collections.Counter(s for (s,) in rows(b, "select set_fqn from item_set_members"))
    out["set_members_changed"] = {k: [ma[k], mb[k]] for k in sorted(set(ma) | set(mb)) if ma[k] != mb[k]}

    # Sources.
    out["source_kinds"] = diff_counts(
        counter(a, "select source_kind, count(*) from item_sources group by 1"),
        counter(b, "select source_kind, count(*) from item_sources group by 1"),
    )
    out["source_labels"] = diff_counts(
        counter(a, "select detail, count(*) from item_sources where source_kind = 'source_label' group by 1"),
        counter(b, "select detail, count(*) from item_sources where source_kind = 'source_label' group by 1"),
    )

    # Stat curves.
    ca = collections.defaultdict(list)
    cb = collections.defaultdict(list)
    for p, h, o, v in rows(a, "select prototype, curve_hash, ordinal, value from stat_curve_values order by prototype, curve_hash, ordinal"):
        ca[(p, h)].append(v)
    for p, h, o, v in rows(b, "select prototype, curve_hash, ordinal, value from stat_curve_values order by prototype, curve_hash, ordinal"):
        cb[(p, h)].append(v)
    out["stat_curves_changed"] = collections.Counter(
        k[0] for k in set(ca) | set(cb) if ca.get(k) != cb.get(k)
    )

    # Relic procs.
    rpa = {r[0]: r[1:] for r in rows(a, "select * from relic_procs")}
    rpb = {r[0]: r[1:] for r in rows(b, "select * from relic_procs")}
    out["relic_procs"] = {"added": sorted(set(rpb) - set(rpa))[:40], "removed": sorted(set(rpa) - set(rpb))[:40],
                          "changed": [[f, rpa[f], rpb[f]] for f in sorted(set(rpa) & set(rpb)) if rpa[f] != rpb[f]][:40]}

    # Singletons.
    ga = dict(rows(a, "select fqn, payload_b64 from singletons"))
    gb = dict(rows(b, "select fqn, payload_b64 from singletons"))
    out["singletons_added"] = sorted(set(gb) - set(ga))
    out["singletons_removed"] = sorted(set(ga) - set(gb))
    out["singletons_changed"] = sorted(f for f in set(ga) & set(gb) if ga[f] != gb[f])

    json.dump(out, sys.stdout, indent=1, default=str)


if __name__ == "__main__":
    main()
