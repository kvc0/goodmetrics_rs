use crate::{gauge::SumHandle, Name};

use super::introspection_factory;

pub(crate) struct LazySumGauge {
    name: Name,
    gauge: std::sync::OnceLock<Option<SumHandle>>,
}

impl LazySumGauge {
    pub(crate) const fn new(name: Name) -> LazySumGauge {
        Self {
            name,
            gauge: std::sync::OnceLock::new(),
        }
    }
}

impl LazySumGauge {
    pub(crate) fn gauge(&self) -> &Option<SumHandle> {
        self.gauge.get_or_init(|| {
            let factory_guard = introspection_factory();
            match factory_guard.as_ref() {
                Some(factory) => {
                    // Though this store races, the factory vends the same gauge via a mutex.
                    let gauge = factory.dimensioned_gauge_sum(
                        "gm_introspect",
                        self.name.clone(),
                        Default::default(),
                    );
                    Some(gauge)
                }
                None => None,
            }
        })
    }
}
