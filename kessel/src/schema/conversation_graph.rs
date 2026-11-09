//! A conversation's node graph (#287). After a conversation's 33-byte preamble lies one GOM
//! object holding every node, so the whole tree reads with the ordinary reader.

use crate::gom_reader::{GomValue, Reader};
use std::collections::{BTreeMap, HashSet};

const EMBEDDED_TAG: u8 = 0x09;
/// Every conversation reads from here to its exact end. Some also read from another offset, by
/// chance, into a frame with a bogus field, so the first offset that reads is not the answer.
const PREAMBLE: usize = 33;

const ROOT_FIELD: u32 = 0x5028_1a20;
const NODES_FIELD: u32 = 0x6683_c66d;
const LINK_NODES_FIELD: u32 = 0x6683_c66e;
const NODE_ID_FIELD: u32 = 0x24c9_78e5;
const CHILDREN_FIELD: u32 = 0x24c9_78b9;
const LINK_TARGET_FIELD: u32 = 0x24cd_478e;
const PLAYER_CHOICE_FIELD: u32 = 0x259e_02f8;
const TEXT_MAP_FIELD: u32 = 0xa787_ee87;
const TEXT_REF_FIELD: u32 = 0x5ce8_7488;
/// `cnvForceRewardType` in the client's dictionary: 1 light side, 2 dark side.
const FORCE_REWARD_FIELD: u32 = 0xa744_d62a;

/// The side of the Force a player choice rewards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Light,
    Dark,
}

impl Alignment {
    pub fn as_str(self) -> &'static str {
        match self {
            Alignment::Light => "lightside",
            Alignment::Dark => "darkside",
        }
    }

    fn of_reward(value: &GomValue) -> Option<Alignment> {
        match value {
            GomValue::Enum(1) => Some(Alignment::Light),
            GomValue::Enum(2) => Some(Alignment::Dark),
            _ => None,
        }
    }
}

/// One node of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub children: Vec<i64>,
    /// The `str.cnv` line id, for a node that speaks.
    pub line_id: Option<u32>,
    pub is_player_choice: bool,
    /// For a link node: the node it jumps to.
    pub link_target: Option<i64>,
    pub alignment: Option<Alignment>,
}

/// A player choice at a decision point.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub line_id: Option<u32>,
    pub alignment: Option<Alignment>,
    /// The first line the choice leads to.
    pub next_line_id: Option<u32>,
}

/// A conversation's nodes by id, with the ids it starts from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConversationGraph {
    pub roots: Vec<i64>,
    pub nodes: BTreeMap<i64, Node>,
}

impl ConversationGraph {
    /// Decode a conversation payload's graph. `None` when it does not read to the payload's end;
    /// a conversation with no node map decodes to no nodes.
    pub fn read(payload: &[u8]) -> Option<ConversationGraph> {
        read_root(payload).map(|root| Self::from_root(&root))
    }

    fn from_root(root: &GomValue) -> ConversationGraph {
        let mut nodes = BTreeMap::new();
        for field in [NODES_FIELD, LINK_NODES_FIELD] {
            let entries = root.embedded_field(field).and_then(GomValue::as_map);
            for (_, node) in entries.into_iter().flatten() {
                if let Some(id) = node
                    .embedded_field(NODE_ID_FIELD)
                    .and_then(GomValue::as_i64)
                {
                    nodes.insert(id, Node::from_value(node));
                }
            }
        }
        let roots = root
            .embedded_field(ROOT_FIELD)
            .map_or_else(Vec::new, children);
        ConversationGraph { roots, nodes }
    }

    /// Every decision point, in ascending id of the node that asks, each with its choices in the
    /// order the game lists them. A lone player line is the character speaking, not a decision,
    /// and a player node with no text is a silent branch the player never sees.
    pub fn decisions(&self) -> Vec<Vec<Choice>> {
        self.nodes
            .values()
            .filter_map(|node| {
                let choices: Vec<Choice> = node
                    .children
                    .iter()
                    .filter_map(|id| self.through_links(*id))
                    .filter(|(_, child)| child.is_player_choice && child.line_id.is_some())
                    .map(|(id, child)| Choice {
                        line_id: child.line_id,
                        alignment: child.alignment,
                        next_line_id: self.next_line(id),
                    })
                    .collect();
                (choices.len() >= 2).then_some(choices)
            })
            .collect()
    }

    /// The node `id` names, following links, as a menu re-offers an earlier option through one.
    /// `None` for a missing node or a loop of links.
    fn through_links(&self, id: i64) -> Option<(i64, &Node)> {
        let mut seen = HashSet::new();
        let mut current = id;
        loop {
            let node = self.nodes.get(&current)?;
            match node.link_target {
                Some(target) if seen.insert(current) => current = target,
                Some(_) => return None,
                None => return Some((current, node)),
            }
        }
    }

    /// The first line after node `from`, depth first in the game's order, through silent nodes
    /// and links.
    fn next_line(&self, from: i64) -> Option<u32> {
        let mut seen = HashSet::from([from]);
        let mut stack: Vec<i64> = self.following(from).into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            match self.nodes.get(&id).and_then(|node| node.line_id) {
                Some(line_id) => return Some(line_id),
                None => stack.extend(self.following(id).into_iter().rev()),
            }
        }
        None
    }

    fn following(&self, id: i64) -> Vec<i64> {
        let Some(node) = self.nodes.get(&id) else {
            return Vec::new();
        };
        let mut next = node.children.clone();
        next.extend(node.link_target);
        next
    }
}

impl Node {
    fn from_value(node: &GomValue) -> Node {
        let line_id = node
            .embedded_field(TEXT_MAP_FIELD)
            .and_then(GomValue::as_map)
            .into_iter()
            .flatten()
            .find_map(|(_, slot)| slot.embedded_field(TEXT_REF_FIELD)?.as_i64())
            .map(|reference| reference as u32);
        Node {
            children: children(node),
            line_id,
            is_player_choice: matches!(
                node.embedded_field(PLAYER_CHOICE_FIELD),
                Some(GomValue::Bool(true))
            ),
            link_target: node
                .embedded_field(LINK_TARGET_FIELD)
                .and_then(GomValue::as_i64),
            alignment: node
                .embedded_field(FORCE_REWARD_FIELD)
                .and_then(Alignment::of_reward),
        }
    }
}

/// The object holding a conversation's nodes, read from after the preamble; `None` when it does
/// not read to the payload's end.
pub(crate) fn read_root(payload: &[u8]) -> Option<GomValue> {
    let mut reader = Reader::new(payload, PREAMBLE);
    let root = reader.read_value(EMBEDDED_TAG).ok()?;
    (reader.pos() == payload.len()).then_some(root)
}

/// A conversation root's dialog nodes (not its link nodes), each with its id.
pub(crate) fn dialog_nodes(root: &GomValue) -> Vec<(i64, &GomValue)> {
    root.embedded_field(NODES_FIELD)
        .and_then(GomValue::as_map)
        .into_iter()
        .flatten()
        .filter_map(|(_, node)| Some((node.embedded_field(NODE_ID_FIELD)?.as_i64()?, node)))
        .collect()
}

fn children(node: &GomValue) -> Vec<i64> {
    node.embedded_field(CHILDREN_FIELD)
        .and_then(GomValue::as_list)
        .into_iter()
        .flatten()
        .filter_map(GomValue::as_i64)
        .collect()
}

#[cfg(test)]
pub(crate) mod testing {
    use crate::gom_reader::GomValue;

    fn number(out: &mut Vec<u8>, n: u64) {
        out.push(0xCF);
        out.extend(n.to_be_bytes());
    }

    fn tag(value: &GomValue) -> u8 {
        match value {
            GomValue::Bool(_) => 0x03,
            GomValue::Enum(_) => 0x05,
            GomValue::Str(_) => 0x06,
            GomValue::List(_) => 0x07,
            GomValue::Map(_) => 0x08,
            GomValue::Embedded(_) => 0x09,
            _ => 0x01,
        }
    }

    /// The bytes of `value`, without its tag: enough of the wire format for these tests.
    pub(crate) fn encode(value: &GomValue, out: &mut Vec<u8>) {
        match value {
            GomValue::U64(n) => number(out, *n),
            GomValue::Bool(b) => out.push(u8::from(*b)),
            GomValue::Enum(i) => out.push(*i as u8 + 1),
            GomValue::Str(s) => {
                out.push(s.len() as u8);
                out.extend(s.as_bytes());
            }
            GomValue::List(items) => {
                let item_tag = items.first().map_or(0x01, tag);
                out.extend([item_tag, items.len() as u8, items.len() as u8]);
                for (i, item) in items.iter().enumerate() {
                    out.push(i as u8);
                    encode(item, out);
                }
            }
            GomValue::Map(pairs) => {
                let value_tag = pairs.first().map_or(0x09, |(_, v)| tag(v));
                out.extend([0x01, value_tag, pairs.len() as u8, pairs.len() as u8]);
                for (k, v) in pairs {
                    encode(k, out);
                    encode(v, out);
                }
            }
            GomValue::Embedded(fields) => {
                out.extend([0, fields.len() as u8]);
                let mut previous = 0;
                for (id, field) in fields {
                    number(out, id - previous);
                    previous = *id;
                    out.push(tag(field));
                    encode(field, out);
                }
            }
            other => panic!("not encoded here: {other:?}"),
        }
    }

    fn ids(ids: &[u64]) -> GomValue {
        GomValue::List(ids.iter().map(|id| GomValue::U64(*id)).collect())
    }

    /// A node with `children`, sorted by field id as the wire needs.
    pub(crate) fn node(
        id: u64,
        children: &[u64],
        line: Option<u64>,
        choice: bool,
        reward: Option<i64>,
    ) -> (GomValue, GomValue) {
        let mut fields = vec![
            (0x24c9_78b9, ids(children)),
            (0x24c9_78e5, GomValue::U64(id)),
        ];
        if let Some(reward) = reward {
            fields.push((0xa744_d62a, GomValue::Enum(reward)));
        }
        if choice {
            fields.push((0x259e_02f8, GomValue::Bool(true)));
        }
        if let Some(line) = line {
            let slot = GomValue::Embedded(vec![(0x5ce8_7488, GomValue::U64(line | 0x11 << 32))]);
            fields.push((0xa787_ee87, GomValue::Map(vec![(GomValue::U64(id), slot)])));
        }
        fields.sort_by_key(|(field, _)| *field);
        (GomValue::U64(id), GomValue::Embedded(fields))
    }

    /// A link node jumping to `target`.
    pub(crate) fn link(id: u64, target: u64) -> (GomValue, GomValue) {
        let fields = vec![
            (0x24c9_78b9, ids(&[])),
            (0x24c9_78e5, GomValue::U64(id)),
            (0x24cd_478e, GomValue::U64(target)),
        ];
        (GomValue::U64(id), GomValue::Embedded(fields))
    }

    /// A conversation payload: a preamble, then one object holding `nodes` from `roots`.
    pub(crate) fn payload(roots: &[u64], nodes: Vec<(GomValue, GomValue)>) -> Vec<u8> {
        let root = GomValue::Embedded(vec![
            (
                0x5028_1a20,
                GomValue::Embedded(vec![(0x24c9_78b9, ids(roots))]),
            ),
            (0x6683_c66d, GomValue::Map(nodes)),
        ]);
        let mut out = vec![0xFF; 33];
        encode(&root, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{link, node, payload};
    use super::*;

    /// 1 asks; 2 and 3 are its choices; 2 goes on through silent 4 to line 30; 3 ends.
    fn decision() -> Vec<u8> {
        payload(
            &[1],
            vec![
                node(1, &[2, 3], Some(10), false, None),
                node(2, &[4], Some(20), true, Some(1)),
                node(3, &[], Some(21), true, Some(2)),
                node(4, &[5], None, false, None),
                node(5, &[], Some(30), false, None),
            ],
        )
    }

    #[test]
    fn the_graph_reads_past_a_preamble_to_the_end() -> anyhow::Result<()> {
        let graph =
            ConversationGraph::read(&decision()).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        assert_eq!(graph.roots, vec![1]);
        assert_eq!(graph.nodes.len(), 5);
        assert_eq!(graph.nodes[&1].children, vec![2, 3]);
        assert_eq!(graph.nodes[&1].line_id, Some(10));
        Ok(())
    }

    #[test]
    fn a_decision_lists_its_choices_with_where_each_leads() -> anyhow::Result<()> {
        let graph =
            ConversationGraph::read(&decision()).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        let decisions = graph.decisions();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0][0].line_id, Some(20));
        assert_eq!(decisions[0][0].next_line_id, Some(30));
        assert_eq!(decisions[0][0].alignment, Some(Alignment::Light));
        assert_eq!(decisions[0][1].next_line_id, None);
        assert_eq!(decisions[0][1].alignment, Some(Alignment::Dark));
        Ok(())
    }

    #[test]
    fn a_loop_in_the_graph_ends_the_search() -> anyhow::Result<()> {
        let looped = payload(
            &[1],
            vec![
                node(1, &[2, 4], Some(10), false, None),
                node(2, &[3], Some(20), true, None),
                node(3, &[2], None, false, None),
                node(4, &[], Some(21), true, None),
            ],
        );
        let graph = ConversationGraph::read(&looped).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        assert_eq!(graph.decisions()[0][0].next_line_id, None);
        Ok(())
    }

    #[test]
    fn a_lone_player_line_is_not_a_decision() -> anyhow::Result<()> {
        let lone = payload(
            &[1],
            vec![
                node(1, &[2], Some(10), false, None),
                node(2, &[], Some(20), true, None),
            ],
        );
        let graph = ConversationGraph::read(&lone).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        assert!(graph.decisions().is_empty());
        Ok(())
    }

    #[test]
    fn a_silent_player_node_is_not_an_option() -> anyhow::Result<()> {
        let silent = payload(
            &[1],
            vec![
                node(1, &[2, 3, 4], Some(10), false, None),
                node(2, &[], Some(20), true, None),
                node(3, &[], None, true, None),
                node(4, &[], Some(21), true, None),
            ],
        );
        let graph = ConversationGraph::read(&silent).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        assert_eq!(graph.decisions()[0].len(), 2);
        Ok(())
    }

    #[test]
    fn a_menu_re_offers_an_earlier_option_through_a_link() -> anyhow::Result<()> {
        // From the hub menu in cnv.alliance.alderaan.broonmark.broonmark: 5 asks, and the first
        // of its two options is reached through link 6.
        let menu = payload(
            &[5],
            vec![
                node(2, &[3], Some(20), true, Some(1)),
                node(3, &[], Some(30), false, None),
                node(4, &[], Some(21), true, None),
                node(5, &[6, 4], Some(10), false, None),
                link(6, 2),
            ],
        );
        let graph = ConversationGraph::read(&menu).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        let decisions = graph.decisions();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0][0].line_id, Some(20));
        assert_eq!(decisions[0][0].alignment, Some(Alignment::Light));
        assert_eq!(decisions[0][0].next_line_id, Some(30));
        assert_eq!(decisions[0][1].line_id, Some(21));
        Ok(())
    }

    #[test]
    fn a_loop_of_links_is_no_option() -> anyhow::Result<()> {
        let looped = payload(
            &[1],
            vec![
                node(1, &[2, 4, 5], Some(10), false, None),
                link(2, 3),
                link(3, 2),
                node(4, &[], Some(20), true, None),
                node(5, &[], Some(21), true, None),
            ],
        );
        let graph = ConversationGraph::read(&looped).ok_or_else(|| anyhow::anyhow!("no graph"))?;
        assert_eq!(graph.decisions()[0].len(), 2);
        Ok(())
    }

    #[test]
    fn a_frame_readable_before_the_preamble_ends_is_not_the_graph() -> anyhow::Result<()> {
        // From offset 33 an empty object stops a byte short; from offset 34 one reads to the end.
        let mut bytes = vec![0xFF; 33];
        bytes.extend([0, 0, 0]);
        assert_eq!(ConversationGraph::read(&bytes), None);
        Ok(())
    }

    #[test]
    fn bytes_that_stop_short_of_the_end_are_no_graph() {
        let mut bytes = decision();
        bytes.push(0x00);
        assert_eq!(ConversationGraph::read(&bytes), None);
    }
}
