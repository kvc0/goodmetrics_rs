use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use crate::pipeline::{DimensionedMeasurementsMap, MeasurementAggregationMap};
use crate::{aggregation::Aggregation, Gauge};
use crate::{pipeline::DimensionPosition, types::Name};

/// Gauges grouped by a shared dimension position
#[derive(Debug, Default)]
pub struct GaugeGroup {
    dimensioned_gauges: HashMap<DimensionPosition, HashMap<Name, Weak<Gauge>>>,
}

impl GaugeGroup {
    /// Put get a shared gauge reference
    pub(crate) fn dimensioned_gauge(
        &mut self,
        name: impl Into<Name>,
        dimensions: DimensionPosition,
        default: fn() -> Gauge,
    ) -> Arc<Gauge> {
        let name = name.into();
        let gauge_position = self.dimensioned_gauges.entry(dimensions).or_default();
        match gauge_position.get(&name) {
            Some(existing) => match existing.upgrade() {
                Some(existing) => existing,
                None => {
                    let gauge: Arc<Gauge> = Arc::new(default());
                    gauge_position.insert(name, Arc::downgrade(&gauge));
                    gauge
                }
            },
            None => {
                let gauge: Arc<Gauge> = Arc::new(default());
                gauge_position.insert(name, Arc::downgrade(&gauge));
                gauge
            }
        }
    }

    /// reset gauge group to 0's, returning the current aggregations
    pub fn reset(&mut self) -> DimensionedMeasurementsMap {
        self.dimensioned_gauges
            .retain(|_dimension_position, gauges| {
                gauges.retain(|_name, gauge| gauge.upgrade().is_some());
                !gauges.is_empty()
            });

        self.dimensioned_gauges
            .iter()
            .filter_map(|(dimension_position, possible_gauges)| {
                let gauges: MeasurementAggregationMap = possible_gauges
                    .iter()
                    .filter_map(|(name, possible_gauge)| {
                        possible_gauge
                            .upgrade()
                            .and_then(|gauge| match gauge.as_ref() {
                                Gauge::StatisticSet(gauge) => gauge.reset().map(|statistic_set| {
                                    (name.to_owned(), Aggregation::StatisticSet(statistic_set))
                                }),
                                Gauge::Sum(gauge) => gauge
                                    .reset()
                                    .map(|sum| (name.to_owned(), Aggregation::Sum(sum))),
                            })
                    })
                    .collect();
                if gauges.is_empty() {
                    None
                } else {
                    Some((dimension_position.to_owned(), gauges))
                }
            })
            .collect()
    }
}
