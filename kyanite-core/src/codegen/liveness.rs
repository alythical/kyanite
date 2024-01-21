use std::collections::{HashMap, HashSet, VecDeque};

use super::{AsmInstr, Instr, Opcode};

#[derive(Debug, Default)]
pub struct Graph<'a> {
    adj: HashMap<usize, Vec<usize>>,
    details: HashMap<usize, &'a AsmInstr>,
}

impl<'a> Graph<'a> {
    pub fn new(instrs: &'a [AsmInstr]) -> Self {
        Self {
            details: instrs.iter().map(|x| (x.id, x)).collect(),
            ..Self::default()
        }
    }

    pub fn add(&mut self, stmt: usize, next: usize) {
        self.adj.entry(stmt).or_default().push(next);
    }

    pub fn temporaries(&self) -> HashSet<String> {
        self.details
            .iter()
            .flat_map(|(_, v)| v.uses().into_iter().chain(v.defines()))
            .filter(|x| x.starts_with('T'))
            .collect()
    }

    fn uses(&self, temp: &String) -> Vec<usize> {
        self.details
            .iter()
            .filter(|(_, v)| v.uses().contains(temp))
            .map(|(&k, _)| k)
            .collect()
    }

    fn defines(&self, cur: usize, temp: &String) -> bool {
        let cur = self.details.get(&cur).unwrap();
        cur.defines().contains(temp)
    }

    fn predecessors(&self, cur: usize) -> impl IntoIterator<Item = usize> + '_ {
        self.adj
            .iter()
            .filter(move |(_, v)| v.contains(&cur))
            .map(|(k, _)| *k)
    }

    pub fn liveness(&self, temp: &String) -> Vec<bool> {
        let mut live = vec![false; self.adj.len()];
        for site in self.uses(temp) {
            live[site] = true;
            let mut worklist = VecDeque::new();
            worklist.push_back(site);
            while !worklist.is_empty() {
                let cur = worklist.pop_front().unwrap();
                if !self.defines(cur, temp) {
                    live[cur] = true;
                    for predecessor in self.predecessors(cur) {
                        worklist.push_back(predecessor);
                    }
                }
            }
        }
        live
    }
}

crate::newtype!(LiveRanges:HashMap<String, Vec<bool>>);

pub type InterferenceGraph = HashMap<String, HashSet<String>>;

impl LiveRanges {
    pub fn get(&self, temp: &str) -> &Vec<bool> {
        let live = self.0.get(temp).unwrap();
        live.iter().enumerate().fold(String::new(), |acc, (i, x)| {
            acc + &format!("{}: {}\n", i, x)
        });
        live
    }

    pub fn interference_graphs(&self, len: usize) -> Vec<InterferenceGraph> {
        (0..len - 1)
            .map(|line| {
                let mut interferences: HashMap<String, HashSet<String>> = HashMap::new();
                let pairs = self
                    .0
                    .iter()
                    .flat_map(|this| self.0.iter().map(move |other| (this, other)))
                    .filter(|((temp, range), (other, other_range))| {
                        temp != other && range[line] && other_range[line]
                    });
                for ((temp, _), (other, _)) in pairs {
                    interferences
                        .entry(temp.to_owned())
                        .or_default()
                        .insert(other.to_owned());
                    interferences
                        .entry(other.to_owned())
                        .or_default()
                        .insert(temp.to_owned());
                }
                interferences
            })
            .collect()
    }
}

impl From<Graph<'_>> for LiveRanges {
    fn from(graph: Graph<'_>) -> Self {
        let ranges = graph
            .temporaries()
            .iter()
            .map(|temp| (temp.to_owned(), graph.liveness(temp)))
            .collect();
        Self(ranges)
    }
}

trait FlowGraphMeta {
    fn defines(&self) -> Vec<String>;
    fn uses(&self) -> Vec<String>;
    fn mov(&self) -> bool;
}

impl FlowGraphMeta for AsmInstr {
    fn defines(&self) -> Vec<String> {
        match &self.inner {
            Instr::Oper { dst, .. } => vec![dst.clone()],
            _ => vec![],
        }
    }

    fn uses(&self) -> Vec<String> {
        match &self.inner {
            Instr::Oper {
                opcode: Opcode::Move,
                src,
                dst,
                ..
            } => vec![src.clone(), dst.clone()],
            Instr::Oper { src, .. } => vec![src.clone()],
            _ => vec![],
        }
    }

    fn mov(&self) -> bool {
        matches!(
            self,
            AsmInstr {
                inner: Instr::Oper {
                    opcode: Opcode::Move,
                    ..
                },
                ..
            }
        )
    }
}

impl<'a> From<&'a Vec<AsmInstr>> for Graph<'a> {
    fn from(instrs: &'a Vec<AsmInstr>) -> Self {
        let mut graph = Self::new(instrs);
        let mut worklist = VecDeque::from(vec![0]);
        let mut visited: HashSet<usize> = HashSet::new();
        while !worklist.is_empty() {
            let stmt = worklist.pop_front().unwrap();
            let instr = &instrs[stmt];
            let successors = if let Some(label) = instr.inner.to() {
                let idx = instrs
                    .iter()
                    .position(|x| x.inner.label().map_or(false, |name| label == *name))
                    .unwrap();
                vec![idx, stmt + 1]
            } else {
                vec![stmt + 1]
            };
            for next in successors {
                graph.add(stmt, next);
                if !visited.contains(&next) && next < instrs.len() - 1 {
                    worklist.push_back(next);
                    visited.insert(next);
                }
            }
        }
        restore(instrs, &mut graph);
        graph
    }
}

fn restore(instrs: &[AsmInstr], graph: &mut Graph) {
    for (&from, to) in graph.adj.iter_mut() {
        if instrs[from].inner.jump() {
            let next = &instrs[from + 1].inner;
            to.retain(|&x| x != from + 1);
            next.label()
                .is_some_and(|x| x == instrs[from].inner.to().unwrap())
                .then(|| {
                    to.push(from + 1);
                });
        }
    }
}
