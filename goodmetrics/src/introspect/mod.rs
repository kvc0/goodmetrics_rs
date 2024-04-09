//! The `introspect` feature.
//! This records and reports metrics about your metrics pipeline.

mod introspection_factory;
mod lazy_gauge;

pub use introspection_factory::run_introspection_metrics;
pub use introspection_factory::IntrospectionConfiguration;

pub(crate) use introspection_factory::introspection_factory;
pub(crate) use lazy_gauge::LazyGauge;
