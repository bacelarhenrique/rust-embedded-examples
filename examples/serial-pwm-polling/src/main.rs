// src/main.rs

/*
    APP     CMD     Length (bytes)      Payload
    0xA0    0x00    0x03                red, green, blue
    0xA0    0x01    0x01                red
    0xA0    0x02    0x01                green
    0xA0    0x03    0x01                blue

    0xB0    0x01    0x00                --
    0xB0    0x02    0x00                --
*/

// std and main are not available for bare metal software
#![no_std]
#![no_main]

use panic_halt as _;

use cortex_m_rt::entry;
use heapless::{consts, Vec};
use nb::block;
use stm32f1xx_hal::{
    gpio::{
        gpiob::{PB6, PB7, PB8, PB9},
        Alternate, PushPull,
    },
    pac::{self, TIM4},
    prelude::*,
    pwm::{Channel, Pwm, C1, C2, C3, C4},
    serial::{Config, Serial, StopBits},
    time::U32Ext,
    timer::{Tim4NoRemap, Timer},
};

struct SerialStruct {
    counter: u8,
    app: u8,
    cmd: u8,
    len: u8,
    data: Vec<u8, consts::U8>,
}

#[entry]
fn main() -> ! {
    // Get access to device peripherals
    let dp = pac::Peripherals::take().unwrap();

    // Get access to RCC, AFIO, FLASH, GPIOA and GPIOB
    let mut rcc = dp.RCC.constrain();
    let mut flash = dp.FLASH.constrain();
    let mut afio = dp.AFIO.constrain(&mut rcc.apb2);
    let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
    let mut gpiob = dp.GPIOB.split(&mut rcc.apb2);

    // Freeze clocks
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    // Set up UART2 pins
    let tx = gpioa.pa2.into_alternate_push_pull(&mut gpioa.crl);
    let rx = gpioa.pa3;

    // Set up TIM4 PWM pins
    let c1 = gpiob.pb6.into_alternate_push_pull(&mut gpiob.crl);
    let c2 = gpiob.pb7.into_alternate_push_pull(&mut gpiob.crl);
    let c3 = gpiob.pb8.into_alternate_push_pull(&mut gpiob.crh);
    let c4 = gpiob.pb9.into_alternate_push_pull(&mut gpiob.crh);
    let pins = (c1, c2, c3, c4);

    // Get PWM instance
    let mut pwm = Timer::tim4(dp.TIM4, &clocks, &mut rcc.apb1).pwm::<Tim4NoRemap, _, _, _>(
        pins,
        &mut afio.mapr,
        1.khz(),
    );

    // Enable clock on each of the channels
    pwm.enable(Channel::C1);
    pwm.enable(Channel::C2);
    pwm.enable(Channel::C3);
    pwm.enable(Channel::C4);

    // Get UART2 instance
    let mut serial = Serial::usart2(
        dp.USART2,
        (tx, rx),
        &mut afio.mapr,
        Config::default()
            .baudrate(9600.bps())
            .stopbits(StopBits::STOP1)
            .parity_none(),
        clocks,
        &mut rcc.apb1,
    );

    // Initialize serial struct
    let mut serial_struct = SerialStruct {
        counter: 0,
        app: 0,
        cmd: 0,
        len: 0,
        data: Vec::new(),
    };

    loop {
        // Poll RX
        let byte_received = block!(serial.read()).unwrap();

        match serial_struct.counter {
            0 => {
                if byte_received == 0xA0 || byte_received == 0xB0 {
                    serial_struct.app = byte_received;
                    serial_struct.counter += 1;
                }
            }
            1 => {
                serial_struct.cmd = byte_received;
                serial_struct.counter += 1;
            }
            2 => {
                serial_struct.len = byte_received;
                serial_struct.counter += 1;
            }
            _ => {
                serial_struct.data.push(byte_received).ok();
                serial_struct.counter += 1;

                if serial_struct.counter == serial_struct.len + 3 {
                    serial_struct.counter = 0;

                    msg_handler(&mut pwm, &mut serial_struct);
                }
            }
        }
    }
}

/// Message handler function
fn msg_handler(
    pwm: &mut Pwm<
        TIM4,
        Tim4NoRemap,
        (C1, C2, C3, C4),
        (
            PB6<Alternate<PushPull>>,
            PB7<Alternate<PushPull>>,
            PB8<Alternate<PushPull>>,
            PB9<Alternate<PushPull>>,
        ),
    >,
    serial_struct: &mut SerialStruct,
) {
    match serial_struct.app {
        0xA0 => {
            // Get max duty cycle and divide it by steps of 255 for the color range
            let step = pwm.get_max_duty() / 255;

            match serial_struct.cmd {
                0x00 => {
                    let red = serial_struct.data[0];
                    let green = serial_struct.data[1];
                    let blue = serial_struct.data[2];

                    pwm.set_duty(Channel::C1, step * red as u16);
                    pwm.set_duty(Channel::C2, step * green as u16);
                    pwm.set_duty(Channel::C3, step * blue as u16);
                }
                0x01 => {
                    let red = serial_struct.data[0];

                    pwm.set_duty(Channel::C1, step * red as u16);
                }
                0x02 => {
                    let green = serial_struct.data[0];

                    pwm.set_duty(Channel::C2, step * green as u16);
                }
                0x03 => {
                    let blue = serial_struct.data[0];

                    pwm.set_duty(Channel::C3, step * blue as u16);
                }
                _ => {}
            }
        }
        0xB0 => match serial_struct.cmd {
            0x01 => {
                let max = pwm.get_max_duty();

                pwm.set_duty(Channel::C4, max / 2);
            }
            0x02 => pwm.set_duty(Channel::C4, 0),
            _ => {}
        },
        _ => {}
    }

    serial_struct.data.clear();
}
