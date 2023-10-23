use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::{
    gauge::{self, StatisticSetGauge},
    pipeline::aggregation::statistic_set::StatisticSet,
    types::Name,
};

#[derive(Default)]
pub struct GaugeGroup {
    gauges: HashMap<Name, Weak<StatisticSetGauge>>,
}

impl GaugeGroup {
    /// Create or retrieve a gauge.
    pub fn gauge(&mut self, name: impl Into<Name>) -> Arc<StatisticSetGauge> {
        let name = name.into();
        match self.gauges.get(&name) {
            Some(existing) => match existing.upgrade() {
                Some(existing) => existing,
                None => {
                    let gauge: Arc<StatisticSetGauge> = Arc::new(gauge::statistic_set_gauge());
                    self.gauges.insert(name, Arc::downgrade(&gauge));
                    gauge
                }
            },
            None => {
                let gauge: Arc<StatisticSetGauge> = Arc::new(gauge::statistic_set_gauge());
                self.gauges.insert(name, Arc::downgrade(&gauge));
                gauge
            }
        }
    }

    pub fn reset(&mut self) -> impl Iterator<Item = (Name, StatisticSet)> + '_ {
        self.gauges.retain(|_name, gauge| gauge.upgrade().is_some());

        self.gauges.iter().filter_map(|(name, possible_gauge)| {
            possible_gauge
                .upgrade()
                .and_then(|gauge| gauge.reset())
                .map(|statistic_set| (name.to_owned(), statistic_set))
        })
    }
}
