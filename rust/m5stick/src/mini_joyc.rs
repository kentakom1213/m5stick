use embedded_hal::i2c::I2c;

const DEFAULT_ADDRESS: u8 = 0x54;
const X_REG: u8 = 0x20;
const Y_REG: u8 = 0x21;
const BUTTON_REG: u8 = 0x30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoyState {
    pub x: i8,
    pub y: i8,
    pub pressed: bool,
}

pub struct MiniJoyC<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> MiniJoyC<I2C>
where
    I2C: I2c,
{
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: DEFAULT_ADDRESS,
        }
    }

    pub fn read(&mut self) -> Result<JoyState, I2C::Error> {
        let x = self.read_register(X_REG)? as i8;
        let y = self.read_register(Y_REG)? as i8;
        let pressed = self.read_register(BUTTON_REG)? == 0;

        Ok(JoyState { x, y, pressed })
    }

    fn read_register(&mut self, register: u8) -> Result<u8, I2C::Error> {
        let mut value = [0u8; 1];

        self.i2c.write(self.address, &[register])?;
        self.i2c.read(self.address, &mut value)?;

        Ok(value[0])
    }
}
