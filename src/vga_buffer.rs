use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;

    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

#[allow(dead_code)]
#[repr(u8)]
pub enum Colour {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGrey = 7,
    DarkGrey = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct ColourCode(u8);

impl ColourCode {
    fn new(foreground: Colour, background: Colour) -> Self {
        Self((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ScreenChar {
    ascii_char: u8,
    colour_code: ColourCode,
}

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Writer {
    cursor_column: usize,
    colour_code: ColourCode,
    buffer: &'static mut Buffer,
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        cursor_column: 0,
        colour_code: ColourCode::new(Colour::White, Colour::Black),
        buffer: unsafe { &mut *(0xB8000 as *mut Buffer) },
    });
}

impl Writer {
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7E | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xFE),
            }
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.cursor_column >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.cursor_column;

                let colour_code = self.colour_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_char: byte,
                    colour_code,
                });
                self.cursor_column += 1;
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let char = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(char);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.cursor_column = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_char: b' ',
            colour_code: self.colour_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

#[test_case]
fn println_not_panic() {
    println!("Simple output");
}

#[test_case]
fn println_not_panic_many() {
    for _ in 0..200 {
        println!("Simple output");
    }
}

#[test_case]
fn println_writes_to_buffer() {
    let s = "A single line to print.";
    x86_64::instructions::interrupts::without_interrupts(|| {
        println!("{}", s);
        for (i, c) in s.chars().enumerate() {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), c);
        }
    });
}

#[test_case]
fn print_can_wrap() {
    let full_line =
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    assert_eq!(full_line.len(), BUFFER_WIDTH);
    x86_64::instructions::interrupts::without_interrupts(|| {
        println!();
        print!("{}", full_line);
        for i in 0..BUFFER_WIDTH {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 1][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), 'x');
        }

        let s = "This should wrap onto a new line!";
        print!("{}", s);
        for i in 0..BUFFER_WIDTH {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), 'x');
        }
        for (i, c) in s.chars().enumerate() {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 1][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), c);
        }
    });
}

#[test_case]
fn can_clear_row() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        println!();
        let s = "A single line to print.";
        print!("{}", s);
        for (i, c) in s.chars().enumerate() {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 1][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), c);
        }

        WRITER.lock().clear_row(BUFFER_HEIGHT - 1);

        for i in 0..BUFFER_WIDTH {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 1][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), ' ');
        }
    });
}

#[test_case]
fn print_with_newlines() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        println!();
        let s = "Line 1\nLine 2";
        print!("{}", s);
        for (i, c) in "Line 1".chars().enumerate() {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), c);
        }
        for (i, c) in "Line 2".chars().enumerate() {
            let buffer_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 1][i].read();
            assert_eq!(char::from(buffer_char.ascii_char), c);
        }
    });
}
