// https://en.hynetek.com/uploadfiles/site/219/news/eb6cc420-847e-40ec-a352-a86fbeedd331.pdf

use std::rc::Rc;

use esp_idf_hal::{
    delay::BLOCK,
    i2c::I2cDriver,
    sys::{
        esp, i2c_ack_type_t_I2C_MASTER_NACK, i2c_cmd_handle_t, i2c_cmd_link_create,
        i2c_cmd_link_delete, i2c_master_cmd_begin, i2c_master_read_byte, i2c_master_start,
        i2c_master_stop, i2c_master_write_byte, i2c_rw_t_I2C_MASTER_READ,
        i2c_rw_t_I2C_MASTER_WRITE,
    },
};
use mycelium_bitfield::enum_from_bits;
use registers::GO_COMMAND;
use serde::Serialize;

const HUSB238_ADDR: u8 = 0x08;

enum_from_bits! {
    #[derive(Debug, PartialEq)]
    pub enum VoltageSelection<u8> {
        PD5V = 0b0001,
        PD9V = 0b0010,
        PD12V = 0b0011,
        PD15V = 0b1000,
        PD18V = 0b1001,
        PD20V = 0b1010
    }
}

enum_from_bits! {
    #[derive(Debug, PartialEq, Serialize)]
    pub enum SrcVoltage<u8> {
        Unattached = 0b0000,
        PD5V = 0b0001,
        PD9V = 0b0010,
        PD12V = 0b0011,
        PD15V = 0b0100,
        PD18V = 0b0101,
        PD20V = 0b0110
    }
}

impl From<SrcVoltage> for f64 {
    fn from(value: SrcVoltage) -> Self {
        match value {
            SrcVoltage::Unattached => f64::NAN,
            SrcVoltage::PD5V => 5.0,
            SrcVoltage::PD9V => 9.0,
            SrcVoltage::PD12V => 12.0,
            SrcVoltage::PD15V => 15.0,
            SrcVoltage::PD18V => 18.0,
            SrcVoltage::PD20V => 20.0,
        }
    }
}

enum_from_bits! {
    #[derive(Debug, PartialEq, Serialize)]
    pub enum Current<u8> {
        PD0_50 = 0b0000,
        PD0_70 = 0b0001,
        PD1_00 = 0b0010,
        PD1_25 = 0b0011,
        PD1_50 = 0b0100,
        PD1_75 = 0b0101,
        PD2_00 = 0b0110,
        PD2_25 = 0b0111,
        PD2_50 = 0b1000,
        PD2_75 = 0b1001,
        PD3_00 = 0b1010,
        PD3_25 = 0b1011,
        PD3_50 = 0b1100,
        PD4_00 = 0b1101,
        PD4_50 = 0b1110,
        PD5_00 = 0b1111
    }
}

impl From<Current> for f64 {
    fn from(value: Current) -> Self {
        match value {
            Current::PD0_50 => 0.5,
            Current::PD0_70 => 0.7,
            Current::PD1_00 => 1.0,
            Current::PD1_25 => 1.25,
            Current::PD1_50 => 1.5,
            Current::PD1_75 => 1.75,
            Current::PD2_00 => 2.0,
            Current::PD2_25 => 2.25,
            Current::PD2_50 => 2.5,
            Current::PD2_75 => 2.75,
            Current::PD3_00 => 3.0,
            Current::PD3_25 => 3.25,
            Current::PD3_50 => 3.5,
            Current::PD4_00 => 4.0,
            Current::PD4_50 => 4.5,
            Current::PD5_00 => 5.0,
        }
    }
}

enum_from_bits! {
    #[derive(Debug, PartialEq)]
    pub enum Command<u8> {
        RequestPdo = 0b00001,
        GetCapabilities = 0b00100,
        HardReset = 0b10000
    }
}

enum_from_bits! {
    #[derive(Debug, PartialEq)]
    pub enum PdoSelection<u8> {
        NotSelected = 0b0000,
        PDO5V = 0b0001,
        PDO9V = 0b0010,
        PDO12V = 0b0011,
        PDO15V = 0b1000,
        PDO18V = 0b1001,
        PDO20V = 0b1010
    }
}

enum_from_bits! {
    #[derive(Debug, PartialEq, Serialize)]
    pub enum Current5V<u8> {
        Default = 0b00,
        Current1_5 = 0b01,
        Current2_4 = 0b10,
        Current3_0 = 0b11
    }
}

enum_from_bits! {
    #[derive(Debug, PartialEq, Serialize)]
    pub enum PdResponse<u8> {
        NoResponse = 0b000,
        Success = 0b001,
        InvalidCommand = 0b011,
        CommandNotSupported = 0b100,
        TransactionFailed = 0b101
    }
}

#[allow(non_camel_case_types)]
pub(super) mod registers {
    use super::*;
    use mycelium_bitfield::bitfield;

    bitfield! {
        pub struct SRC_PDO<u8> {
            pub const _RESERVED = 4;
            pub const PDO_SELECTION: PdoSelection;
        }
    }

    impl SRC_PDO {
        pub const ADDR: u8 = 0x08;
    }

    bitfield! {
        pub struct GO_COMMAND<u8> {
            pub const FUNCTION: Command;
            pub const _RESERVED = 3;
        }
    }

    impl GO_COMMAND {
        pub const ADDR: u8 = 0x09;
    }

    bitfield! {
        pub struct SRC_PDO_STATUS<u8> {
            pub const CURRENT: Current;
            pub const _PAD = 3;
            pub const DETECTED: bool;
        }
    }

    impl SRC_PDO_STATUS {
        pub const ADDR_5V: u8 = 0x02;
        pub const ADDR_9V: u8 = 0x03;
        pub const ADDR_12V: u8 = 0x04;
        pub const ADDR_15V: u8 = 0x05;
        pub const ADDR_18V: u8 = 0x06;
        pub const ADDR_20V: u8 = 0x07;
    }

    bitfield! {
        pub struct PD_STATUS0<u8> {
            pub const SRC_CURRENT: Current;
            pub const SRC_VOLTAGE: SrcVoltage;
        }
    }

    impl PD_STATUS0 {
        pub const ADDR: u8 = 0x00;
    }

    bitfield! {
        pub struct PD_STATUS1<u8> {
            pub const CURRENT_5V: Current5V;
            pub const IS_5V: bool;
            pub const PD_RESPONSE: PdResponse;
            pub const IS_ATTACHED: bool;
            pub const CC2_CONNECTED: bool;
        }
    }

    impl PD_STATUS1 {
        pub const ADDR: u8 = 0x01;
    }
}

#[derive(Clone)]
pub struct Husb238Driver {
    pub i2c: Rc<parking_lot::Mutex<I2cDriver<'static>>>,
}

struct CommandLink(i2c_cmd_handle_t);

impl Drop for CommandLink {
    fn drop(&mut self) {
        unsafe {
            i2c_cmd_link_delete(self.0);
        }
    }
}

impl Husb238Driver {
    pub fn read_register<T: From<u8>>(&mut self, reg: u8) -> anyhow::Result<T> {
        let i2c = self.i2c.lock();
        let cmd_link = CommandLink(unsafe { i2c_cmd_link_create() });

        esp!(unsafe { i2c_master_start(cmd_link.0) })?;
        esp!(unsafe {
            i2c_master_write_byte(
                cmd_link.0,
                (HUSB238_ADDR << 1) | (i2c_rw_t_I2C_MASTER_WRITE as u8),
                true,
            )
        })?;
        esp!(unsafe { i2c_master_write_byte(cmd_link.0, reg, true) })?;

        esp!(unsafe { i2c_master_start(cmd_link.0) })?;
        esp!(unsafe {
            i2c_master_write_byte(
                cmd_link.0,
                (HUSB238_ADDR << 1) | (i2c_rw_t_I2C_MASTER_READ as u8),
                true,
            )
        })?;

        let mut data = 0u8;

        esp!(unsafe {
            i2c_master_read_byte(
                cmd_link.0,
                (&mut data) as *mut u8,
                i2c_ack_type_t_I2C_MASTER_NACK,
            )
        })?;
        esp!(unsafe { i2c_master_stop(cmd_link.0) })?;

        esp!(unsafe { i2c_master_cmd_begin(i2c.port(), cmd_link.0, BLOCK) })?;
        drop(cmd_link);

        Ok(data.into())
    }

    pub fn write_register(&mut self, reg: u8, val: u8) -> anyhow::Result<()> {
        let i2c = self.i2c.lock();
        let cmd_link = CommandLink(unsafe { i2c_cmd_link_create() });

        esp!(unsafe { i2c_master_start(cmd_link.0) })?;
        esp!(unsafe {
            i2c_master_write_byte(
                cmd_link.0,
                (HUSB238_ADDR << 1) | (i2c_rw_t_I2C_MASTER_WRITE as u8),
                true,
            )
        })?;
        esp!(unsafe { i2c_master_write_byte(cmd_link.0, reg, true) })?;
        esp!(unsafe { i2c_master_write_byte(cmd_link.0, val, true) })?;
        esp!(unsafe { i2c_master_stop(cmd_link.0) })?;

        esp!(unsafe { i2c_master_cmd_begin(i2c.port(), cmd_link.0, BLOCK) })?;
        drop(cmd_link);

        Ok(())
    }

    pub fn write_command(&mut self, command: Command) -> anyhow::Result<()> {
        self.write_register(
            GO_COMMAND::ADDR,
            GO_COMMAND::new().with(GO_COMMAND::FUNCTION, command).bits(),
        )
    }
}
