//! Stats are more statically allocated metrics. Something like gauges, but faster and with less framework and less boilerplate.

mod stat;

pub use stat::{Collector, ObservedNumber, ResettingNumber, Stat};
