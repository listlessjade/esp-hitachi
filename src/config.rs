use std::fs::File;

use serde::{de::DeserializeOwned, Serialize};

pub trait ConfigType: Serialize + DeserializeOwned {
    const PATH: &str;
    const HAS_DEFAULT: bool;

    fn store(&self) -> anyhow::Result<()> {
        let f = File::create(Self::PATH)?;
        serde_json::to_writer(f, self)?;
        Ok(())
    }

    fn read() -> anyhow::Result<Option<Self>> {
        if !std::fs::exists(Self::PATH)? { 
            if Self::HAS_DEFAULT {
                let def = Self::default().unwrap();
                def.store()?;
                return Ok(Some(def))
            }
            return Ok(None) };
        let f = File::open(Self::PATH)?;
        Ok(Some(serde_json::from_reader(f)?))
    }


    fn default() -> Option<Self> { None }
}
