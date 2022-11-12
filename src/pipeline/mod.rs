pub mod aggregating_sink;
pub mod logging_sink;
pub mod serializing_sink;

pub trait Sink<Sunk> {
    fn accept(&self, to_sink: Sunk);
}
