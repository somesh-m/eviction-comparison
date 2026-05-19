pub mod algorithms;
pub trait Cache {
    fn upsert(&mut self, key: String, value: String);
    fn get(&mut self, key: &str) -> Option<String>;
    fn name(&self) -> &str;
    fn stats(&mut self);
    fn debug_integrity(&mut self);
}
