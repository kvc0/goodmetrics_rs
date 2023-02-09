mod goodmetrics;
mod lightstep;

use crate::goodmetrics::goodmetrics_demo;
use criterion::{criterion_group, criterion_main};
use lightstep::lightstep_demo;

criterion_group!(lightstep, lightstep_demo);
criterion_group!(goodmetrics, goodmetrics_demo);
criterion_main!(goodmetrics);
