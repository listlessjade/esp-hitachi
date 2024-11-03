use anyhow::anyhow;
use i2c::{
    registers::{GO_COMMAND, PD_STATUS0, PD_STATUS1, SRC_PDO, SRC_PDO_STATUS},
    Current, Current5V, PdResponse, PdoSelection, SrcVoltage, VoltageSelection,
};
pub mod i2c;
pub use i2c::Husb238Driver;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub selected_voltage: SrcVoltage,
    pub selected_current: Current,
    pub attached: bool,
    pub cc2_attached: bool,
    pub pd_response: PdResponse,
    pub current_5v: Option<Current5V>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Capabilities {
    pub pdo_5v: Option<Current>,
    pub pdo_9v: Option<Current>,
    pub pdo_12v: Option<Current>,
    pub pdo_15v: Option<Current>,
    pub pdo_18v: Option<Current>,
    pub pdo_20v: Option<Current>,
}

impl Husb238Driver {
    pub fn get_status(&mut self) -> anyhow::Result<Status> {
        let status0: PD_STATUS0 = self.read_register(PD_STATUS0::ADDR)?;
        let status1: PD_STATUS1 = self.read_register(PD_STATUS1::ADDR)?;

        Ok(Status {
            selected_voltage: status0.get(PD_STATUS0::SRC_VOLTAGE),
            selected_current: status0.get(PD_STATUS0::SRC_CURRENT),
            attached: status1.get(PD_STATUS1::IS_ATTACHED),
            cc2_attached: status1.get(PD_STATUS1::CC2_CONNECTED),
            pd_response: status1.get(PD_STATUS1::PD_RESPONSE),
            current_5v: if status1.get(PD_STATUS1::IS_5V) {
                Some(status1.get(PD_STATUS1::CURRENT_5V))
            } else {
                None
            },
        })
    }

    pub fn voltage_src_status(
        &mut self,
        voltage: VoltageSelection,
    ) -> anyhow::Result<Option<Current>> {
        let register = match voltage {
            VoltageSelection::PD5V => SRC_PDO_STATUS::ADDR_5V,
            VoltageSelection::PD9V => SRC_PDO_STATUS::ADDR_9V,
            VoltageSelection::PD12V => SRC_PDO_STATUS::ADDR_12V,
            VoltageSelection::PD15V => SRC_PDO_STATUS::ADDR_15V,
            VoltageSelection::PD18V => SRC_PDO_STATUS::ADDR_18V,
            VoltageSelection::PD20V => SRC_PDO_STATUS::ADDR_20V,
        };

        let status: SRC_PDO_STATUS = self.read_register(register)?;

        if status.get(SRC_PDO_STATUS::DETECTED) {
            Ok(Some(status.get(SRC_PDO_STATUS::CURRENT)))
        } else {
            Ok(None)
        }
    }

    pub fn select_pdo(&mut self, select: &str) -> anyhow::Result<()> {
        let selection = match select.to_ascii_uppercase().as_str() {
            "5V" => PdoSelection::PDO5V,
            "9V" => PdoSelection::PDO9V,
            "12V" => PdoSelection::PDO12V,
            _ => return Err(anyhow!("Unsupported voltage: only [5V, 9V, 12V] allowed.")),
        };

        println!("{:?}", selection);

        self.write_register(SRC_PDO::ADDR, 0b0010)?;
        self.write_register(GO_COMMAND::ADDR, 1)?;
        // self.write_command(i2c::Command::RequestPdo).map_err(rhai_anyhow)?;

        Ok(())
    }

    pub fn get_capabilities(&mut self) -> anyhow::Result<Capabilities> {
        self.write_command(i2c::Command::GetCapabilities)?;
        Ok(Capabilities {
            pdo_5v: self.voltage_src_status(VoltageSelection::PD5V)?,
            pdo_9v: self.voltage_src_status(VoltageSelection::PD9V)?,
            pdo_12v: self.voltage_src_status(VoltageSelection::PD12V)?,
            pdo_15v: self.voltage_src_status(VoltageSelection::PD15V)?,
            pdo_18v: self.voltage_src_status(VoltageSelection::PD18V)?,
            pdo_20v: self.voltage_src_status(VoltageSelection::PD20V)?,
        })
    }
}
