//! Port of `worldengine/step.py`.
//!
//! A step in the world generation process. The process starts with the plates
//! simulation and goes on through intermediate steps to reach the `Full` step.
//!
//! Note the Python's `Step.plates()` sets the same three flags as `full()` —
//! preserved here rather than "fixed", since callers depend on the behaviour.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Step {
    Plates,
    Precipitations,
    #[default]
    Full,
}

impl Step {
    pub fn get_by_name(name: &str) -> Result<Step, String> {
        match name {
            "plates" => Ok(Step::Plates),
            "precipitations" => Ok(Step::Precipitations),
            "full" => Ok(Step::Full),
            other => Err(format!("Unknown step '{other}'")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Step::Plates => "plates",
            Step::Precipitations => "precipitations",
            Step::Full => "full",
        }
    }

    pub fn include_plates(self) -> bool {
        true
    }

    pub fn include_precipitations(self) -> bool {
        // `precipitations`, `full` and (per the original) `plates` all set this.
        true
    }

    pub fn include_erosion(self) -> bool {
        matches!(self, Step::Plates | Step::Full)
    }

    pub fn include_biome(self) -> bool {
        matches!(self, Step::Plates | Step::Full)
    }
}
