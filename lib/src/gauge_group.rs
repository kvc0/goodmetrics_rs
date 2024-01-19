use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::pipeline::aggregation::Aggregation;
use crate::pipeline::aggregator::DimensionedMeasurementsMap;
use crate::{
    gauge::{self, StatisticSetGauge},
    pipeline::aggregator::DimensionPosition,
    types::Name,
};

#[derive(Default)]
pub struct GaugeGroup {
    pub(crate) dimensioned_gauges:
        HashMap<DimensionPosition, HashMap<Name, Weak<StatisticSetGauge>>>,
}

impl GaugeGroup {
    /// Create or retrieve a gauge.
    pub fn gauge(&mut self, name: impl Into<Name>) -> Arc<StatisticSetGauge> {
        self.dimensioned_gauge(name, Default::default())
    }

    pub fn dimensioned_gauge(
        &mut self,
        name: impl Into<Name>,
        dimensions: DimensionPosition,
    ) -> Arc<StatisticSetGauge> {
        let name = name.into();
        let gauge_position = self.dimensioned_gauges.entry(dimensions).or_default();
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

    // reset gauge group, aggregating and returning previous values
    pub fn reset(&mut self) -> DimensionedMeasurementsMap {
        self.dimensioned_gauges
            .retain(|_dimension_position, gauges| {
                gauges.retain(|_name, gauge| gauge.upgrade().is_some());
                !gauges.is_empty()
            });

        self.dimensioned_gauges
            .iter()
            .map(|(dimension_position, gauges)| {
                (
                    dimension_position.to_owned(),
                    gauges
                        .iter()
                        .filter_map(|(name, possible_gauge)| {
                            possible_gauge
                                .upgrade()
                                .and_then(|gauge| gauge.reset())
                                .map(|statistic_set| {
                                    (name.to_owned(), Aggregation::StatisticSet(statistic_set))
                                })
                        })
                        .collect(),
                )
            })
            .collect()
    }
}
