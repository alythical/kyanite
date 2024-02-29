use crate::backend::kyir::{
    alloc::liveness::LiveRanges,
    arch::{ArchInstr, Frame},
};
use std::collections::{HashMap, HashSet};

pub struct Color<I: ArchInstr, F: Frame<I>> {
    interferences: HashMap<String, HashSet<String>>,
    live: Vec<HashMap<String, HashSet<String>>>,
    _phantom: std::marker::PhantomData<(F, I)>,
}

impl<I: ArchInstr, F: Frame<I>> Color<I, F> {
    pub fn new(
        interferences: HashMap<String, HashSet<String>>,
        live: Vec<HashMap<String, HashSet<String>>>,
    ) -> Self {
        Self {
            interferences,
            live,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn color(&self, ranges: &LiveRanges) -> HashMap<String, String> {
        let mut colors = HashMap::new();
        let temporaries: Vec<_> = ranges.keys().collect();
        let registers: Vec<String> = F::registers()
            .temporary
            .iter()
            .map(|&reg| String::from(reg))
            .collect();
        for (line, graph) in self.live.iter().enumerate() {
            let mut live: Vec<&String> = temporaries
                .iter()
                .filter(|&t| ranges.get(t)[line])
                .copied()
                .collect();
            live.sort_by_key(|&t| graph.get(t).map_or(0, HashSet::len));
            while let Some(temp) = live.pop() {
                if !colors.contains_key(temp) {
                    let interferes = &self.interferences[temp];
                    log::trace!("{temp} interferes with {interferes:?}");
                    let used: Vec<_> = interferes.iter().map(|t| colors.get(t)).collect();
                    let color = registers
                        .iter()
                        .find(|&r| !used.contains(&Some(r)))
                        .expect("ran out of registers");
                    colors.insert(temp.clone(), color.clone());
                }
            }
        }
        log::trace!("register mapping: {colors:#?}");
        colors
    }
}
