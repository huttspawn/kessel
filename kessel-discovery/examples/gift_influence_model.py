"""Test gift influence formulas against the community per-bracket gift counts
(Love, Legacy Altruism +30%) carried in huttspawn site/src/lib/gifts/curve.ts."""
import math, struct
def f32(x): return struct.unpack('f', struct.pack('f', x))[0]
Q = {'premium': 8.0, 'prototype': 16.0, 'artifact': 32.0, 'legendary': 80.0}   # itmGiftQualityMap enum 2..5
LOVE = 16.0                                                                    # itmGiftCategoryInterestMap enum 5
DR = {0: 1.0, 1: f32(1.2), 2: f32(1.4), 3: f32(1.6), -1: f32(0.8), -2: f32(0.6), -3: f32(0.4), -4: f32(0.2), -5: f32(0.1)}
def dr(d): return DR[max(-5, min(3, d))]
RANKMAP = [-1000, 3600, 10000, 22500, 40000, 90000, 160000, 250000, 360000]  # chrCompanionAffectionRankMap
BL = [1, 6, 10, 15, 20, 30, 40, 50]
def infl(L): return 0 if L <= 1 else L * L * 100                              # chrAllianceContactInfluenceLevels
def crank(points):
    return max(i for i, t in enumerate(RANKMAP) if points >= t)
T = {1: {'premium': [22, 49, 125, 264, 1480, 4143, None], 'prototype': [11, 25, 63, 132, 755, 2072, None], 'artifact': [6, 13, 32, 66, 378, 1056, None], 'legendary': [3, 5, 13, 27, 151, 421, None]},
     2: {'premium': [18, 39, 95, 175, 755, 2072, 5326], 'prototype': [10, 20, 47, 88, 378, 1056, 2663], 'artifact': [5, 10, 24, 44, 188, 528, 1358], 'legendary': [2, 4, 10, 18, 76, 211, 541]},
     3: {'premium': [16, 32, 76, 132, 500, 1056, 2663], 'prototype': [8, 17, 38, 66, 250, 528, 1358], 'artifact': [4, 9, 19, 33, 126, 263, 679], 'legendary': [2, 4, 8, 14, 51, 106, 271]},
     4: {'premium': [14, 28, 63, 106, 378, 700, 1358], 'prototype': [7, 14, 32, 53, 188, 350, 679], 'artifact': [4, 7, 16, 27, 94, 176, 338], 'legendary': [2, 3, 7, 11, 38, 71, 136]},
     5: {'premium': [14, 25, 54, 88, 301, 528, 900], 'prototype': [7, 13, 27, 44, 151, 263, 450], 'artifact': [4, 7, 14, 22, 76, 132, 226], 'legendary': [2, 3, 6, 9, 31, 53, 91]},
     6: {'premium': [14, 25, 47, 76, 250, 421, 679], 'prototype': [7, 13, 24, 38, 126, 211, 338], 'artifact': [4, 7, 12, 19, 63, 106, 169], 'legendary': [2, 3, 5, 8, 26, 43, 68]}}
models = {
  'raw*1.3': lambda q, d: q * LOVE * dr(d) * 1.3,
  'round(raw)*1.3': lambda q, d: round(q * LOVE * dr(d)) * 1.3,
  'floor(raw)*1.3': lambda q, d: math.floor(q * LOVE * dr(d)) * 1.3,
  'ceil(raw)*1.3': lambda q, d: math.ceil(q * LOVE * dr(d) - 1e-9) * 1.3,
  'round(round(raw)*1.3)': lambda q, d: round(round(q * LOVE * dr(d)) * 1.3),
  'floor(round(raw)*1.3)': lambda q, d: math.floor(round(q * LOVE * dr(d)) * 1.3),
}
for off in (0, 1):
  for name, fn in models.items():
    hit = miss = 0; bad = []
    for r, row in T.items():
        for q, counts in row.items():
            for b, c in enumerate(counts):
                if c is None: continue
                start = infl(BL[b]); span = infl(BL[b + 1]) - start
                d = (r - off) - crank(start)
                per = fn(Q[q], d)
                n = math.ceil(span / per - 1e-9)
                if n == c: hit += 1
                else: miss += 1; bad.append((r, q, BL[b], c, n, round(per, 2)))
    print(f'giftrank-{off} {name:24s} hit {hit} miss {miss}', bad[:8])
