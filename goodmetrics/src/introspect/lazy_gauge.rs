use std::sync::Arc;

use arc_swap::ArcSwapOption;

use crate::{Gauge, Name};

use super::introspection_factory;

pub(crate) struct LazyGauge {
    name: Name,
    gauge: ArcSwapOption<Gauge>,
}

impl LazyGauge {
    pub(crate) const fn new(name: Name) -> LazyGauge {
        Self {
            name,
            gauge: ArcSwapOption::const_empty(),
        }
    }
}

impl LazyGauge {
    pub(crate) fn gauge(&self) -> Option<Arc<Gauge>> {
        let gauge_guard = self.gauge.load();
        if gauge_guard.is_none() {
            let factory_guard = introspection_factory();
            match factory_guard.as_ref() {
                Some(factory) => {
                    // Though this store races, the factory vends the same gauge via a mutex.
                    let gauge = factory.dimensioned_gauge_sum(
                        "gm_introspect",
                        self.name.clone(),
                        Default::default(),
                    );
                    self.gauge.store(Some(gauge.clone()));
                    Some(gauge)
                }
                None => None,
            }
        } else {
            match gauge_guard.as_ref() {
                Some(gauge) => Some(gauge.clone()),
                None => {
                    log::error!("this was checked for none");
                    None
                }
            }
        }
    }
}
