use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadTemperature {
    // ---
    pub unit: TemperatureUnit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadHumidity;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadPressure;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    // ---
    pub value: f32,
    pub unit: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TemperatureUnit {
    Celsius,
    Fahrenheit,
}
