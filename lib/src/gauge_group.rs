use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::{
    gauge::{self, StatisticSetGauge},
    pipeline::{aggregation::statistic_set::StatisticSet, aggregator::DimensionPosition},
    types::Name,
};

#[derive(Default)]
pub struct GaugeGroup {
    gauges: HashMap<DimensionPosition, HashMap<Name, Weak<StatisticSetGauge>>>,
}

impl GaugeGroup {
    /// Create or retrieve a gauge.
    pub fn gauge(&mut self, name: impl Into<Name>) -> Arc<StatisticSetGauge> {
        self.dimensioned_gauge(name, Default::default())
    }

    pub fn dimensioned_gauge(&mut self, name: impl Into<Name>, dimensions: DimensionPosition) -> Arc<StatisticSetGauge> {
        let name = name.into();
        let mut gauge_position = self.gauges.entry(dimensions).or_default();
        match gauge_position.get(&name) {
            Some(existing) => match existing.upgrade() {
                Some(existing) => existing,
                None => {
                    let gauge: Arc<StatisticSetGauge> = Arc::new(gauge::statistic_set_gauge());
                    gauge_position.insert(name, Arc::downgrade(&gauge));
                    gauge
                }
            },
            None => {
                let gauge: Arc<StatisticSetGauge> = Arc::new(gauge::statistic_set_gauge());
                gauge_position.insert(name, Arc::downgrade(&gauge));
                gauge
            }
        }
    }

    pub fn reset(&mut self) -> (impl Iterator<Item = DimensionedAggregatedGauges<StatisticSet>> + '_) {
        self.gauges.retain(|_name, gauge| gauge.upgrade().is_some());

        self.gauges.iter().filter_map(|(name, possible_gauge)| {
            possible_gauge
                .upgrade()
                .and_then(|gauge| gauge.reset())
                .map(|statistic_set| (name.to_owned(), statistic_set))
        });
        vec![].into_iter()
    }
}

pub struct DimensionedAggregatedGauges<MeasurementType> {
    position: DimensionPosition,
    gauges: Vec<AggregatedGauge<MeasurementType>>,
}

pub struct AggregatedGauge<MeasurementType> {
    pub name: Name,
    measurement: MeasurementType,
}
