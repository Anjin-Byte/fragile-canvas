use core::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

pub mod bit_twiddling_utils {
    pub fn set_bit(value: u8, bit: u8) -> u8 {
        value | (1 << bit)
    }

    pub fn reset_bit(value: u8, bit: u8) -> u8 {
        value & !(1 << bit)
    }

    pub fn toggle_bit(value: u8, bit: u8) -> u8 {
        value ^ (1 << bit)
    }

    pub fn is_bit_set(value: u8, bit: u8) -> bool {
        (value & (1 << bit)) != 0
    }
}

#[derive(Debug, Clone)]
enum RegisterType {
    PC,
    SP,
    A,
    F,
    B,
    C,
    D,
    E,
    H,
    L,
    AF,
    BC,
    DE,
    HL,
    IR,
    IE,
}

pub enum Flag {
    Zero = 7,
    Subtract = 6,
    HalfCarry = 5,
    Carry = 4,
}

pub enum Interrupt {
    VBlank = 0,
    LCDStat = 1,
    Timer = 2,
    Serial = 3,
    Joypad = 4,
}

#[derive(Clone)]
struct RegisterFile {
    pc: u16,
    sp: u16,
    a:  u8,
    f:  u8,
    b:  u8,
    c:  u8,
    d:  u8,
    e:  u8,
    h:  u8,
    l:  u8,
    ir: u8,
    ie: u8,
}

impl fmt::Display for RegisterFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[PC: 0x")?;
        write!(f, "{:#^04X}", self.read_register(&RegisterType::PC))?;
        f.write_str(" | SP: 0x")?;
        write!(f, "{:#^04X}", self.read_register(&RegisterType::SP))?;
        f.write_str(" | AF: 0x")?;
        write!(f, "{:#^04X}", self.read_register(&RegisterType::AF))?;
        f.write_str(" | BC: 0x")?;
        write!(f, "{:#^04X}", self.read_register(&RegisterType::BC))?;
        f.write_str(" | DE: 0x")?;
        write!(f, "{:#^04X}", self.read_register(&RegisterType::DE))?;
        f.write_str(" | HL: 0x")?;
        write!(f, "{:#^04X}", self.read_register(&RegisterType::HL))?;
        f.write_str(" | IR: 0x")?;
        write!(f, "{:#^02X}", self.read_register(&RegisterType::IR))?;
        f.write_str(" | IE: 0x")?;
        write!(f, "{:#^02X}", self.read_register(&RegisterType::IE))?;
        f.write_str("]")
    }
}

impl RegisterFile {
    fn new() -> Self {
        Self {
            pc: 0,
            sp: 0,
            a:  0,
            f:  0,
            b:  0,
            c:  0,
            d:  0,
            e:  0,
            h:  0,
            l:  0,
            ir: 0,
            ie: 0,
        }
    }

    // General helper for 8-bit register access
    fn get_8bit(&self, register: &RegisterType) -> u8 {
        match register {
            RegisterType::A => self.a,
            RegisterType::F => self.f,
            RegisterType::B => self.b,
            RegisterType::C => self.c,
            RegisterType::D => self.d,
            RegisterType::E => self.e,
            RegisterType::H => self.h,
            RegisterType::L => self.l,
            RegisterType::IR => self.ir,
            RegisterType::IE => self.ie,
            _ => panic!("Invalid 8-bit register: {:?}", register),
        }
    }

    fn set_8bit(&mut self, register: &RegisterType, value: u8) {
        match register {
            RegisterType::A => self.a = value,
            RegisterType::F => self.f = value,
            RegisterType::B => self.b = value,
            RegisterType::C => self.c = value,
            RegisterType::D => self.d = value,
            RegisterType::E => self.e = value,
            RegisterType::H => self.h = value,
            RegisterType::L => self.l = value,
            RegisterType::IR => self.ir = value,
            RegisterType::IE => self.ie = value,
            _ => panic!("Invalid 8-bit register: {:?}", register),
        }
    }

    // General helper for 16-bit register access
    fn get_16bit(&self, register: &RegisterType) -> u16 {
        match register {
            RegisterType::PC => self.pc,
            RegisterType::SP => self.sp,
            RegisterType::AF => u16::from_be_bytes([self.a, self.f]),
            RegisterType::BC => u16::from_be_bytes([self.b, self.c]),
            RegisterType::DE => u16::from_be_bytes([self.d, self.e]),
            RegisterType::HL => u16::from_be_bytes([self.h, self.l]),
            _ => panic!("Invalid 16-bit register: {:?}", register),
        }
    }

    fn set_16bit(&mut self, register: &RegisterType, value: u16) {
        match register {
            RegisterType::PC => self.pc = value,
            RegisterType::SP => self.sp = value,
            RegisterType::AF => {
                let [high, low] = value.to_be_bytes();
                self.a = high;
                self.f = low;
            }
            RegisterType::BC => {
                let [high, low] = value.to_be_bytes();
                self.b = high;
                self.c = low;
            }
            RegisterType::DE => {
                let [high, low] = value.to_be_bytes();
                self.d = high;
                self.e = low;
            }
            RegisterType::HL => {
                let [high, low] = value.to_be_bytes();
                self.h = high;
                self.l = low;
            }
            _ => panic!("Invalid 16-bit register: {:?}", register),
        }
    }

    pub fn read_register(&self, register: &RegisterType) -> u16 {
        match register {
            // 8-bit registers
            RegisterType::A
            | RegisterType::F
            | RegisterType::B
            | RegisterType::C
            | RegisterType::D
            | RegisterType::E
            | RegisterType::H
            | RegisterType::L
            | RegisterType::IR
            | RegisterType::IE => {
                self.get_8bit(register) as u16
            }

            // 16-bit registers
            RegisterType::PC 
            | RegisterType::SP 
            | RegisterType::AF 
            | RegisterType::BC 
            | RegisterType::DE 
            | RegisterType::HL => {
                self.get_16bit(register)
            }
        }
    }

    pub fn write_register(&mut self, register: &RegisterType, value: u16) {
        match register {
            // 8-bit registers
            RegisterType::A
            | RegisterType::F
            | RegisterType::B
            | RegisterType::C
            | RegisterType::D
            | RegisterType::E
            | RegisterType::H
            | RegisterType::L
            | RegisterType::IR
            | RegisterType::IE => {
                self.set_8bit(register, value as u8)
            }

            // 16-bit registers
            RegisterType::PC 
            | RegisterType::SP 
            | RegisterType::AF 
            | RegisterType::BC 
            | RegisterType::DE 
            | RegisterType::HL => {
                self.set_16bit(register, value)
            }
        }
    }

    pub fn modify_register<F>(&mut self, register: &RegisterType, f: F)
    where
        F: FnOnce(u16) -> u16,
    {
        let current_value = self.read_register(register);
        let new_value = f(current_value);
        self.write_register(register, new_value);
    }

    pub fn increment_reg(&mut self, reg: &RegisterType) {
        match reg {
            // 8-bit registers
            RegisterType::A
            | RegisterType::F
            | RegisterType::B
            | RegisterType::C
            | RegisterType::D
            | RegisterType::E
            | RegisterType::H
            | RegisterType::L
            | RegisterType::IR
            | RegisterType::IE => {
                let value = self.get_8bit(reg).wrapping_add(1);
                self.set_8bit(reg, value);
            }

            // 16-bit registers
            RegisterType::PC | 
            RegisterType::SP | 
            RegisterType::AF | 
            RegisterType::BC | 
            RegisterType::DE | 
            RegisterType::HL => {
                let value = self.get_16bit(reg).wrapping_add(1);
                self.set_16bit(reg, value);
            }
        }
    }

    pub fn decrement_reg(&mut self, reg: &RegisterType) {
        match reg {
            // 8-bit registers
            RegisterType::A
            | RegisterType::F
            | RegisterType::B
            | RegisterType::C
            | RegisterType::D
            | RegisterType::E
            | RegisterType::H
            | RegisterType::L
            | RegisterType::IR
            | RegisterType::IE => {
                let value = self.get_8bit(reg).wrapping_sub(1);
                self.set_8bit(reg, value);
            }

            // 16-bit registers
            RegisterType::PC | 
            RegisterType::SP | 
            RegisterType::AF | 
            RegisterType::BC | 
            RegisterType::DE | 
            RegisterType::HL => {
                let value = self.get_16bit(reg).wrapping_sub(1);
                self.set_16bit(reg, value);
            }
        }
    }

    fn set_register_bit(&mut self, register: RegisterType, bit: u8) {
        match register {
            RegisterType::A => self.a = bit_twiddling_utils::set_bit(self.a, bit),
            RegisterType::F => self.f = bit_twiddling_utils::set_bit(self.f, bit),
            RegisterType::IR => self.ir = bit_twiddling_utils::set_bit(self.ir, bit),
            RegisterType::IE => self.ie = bit_twiddling_utils::set_bit(self.ie, bit),
            _ => panic!("Bitwise operation 'set' not allowed on register: {:?}", register),
        }
    }

    fn reset_register_bit(&mut self, register: RegisterType, bit: u8) {
        match register {
            RegisterType::A => self.a = bit_twiddling_utils::reset_bit(self.a, bit),
            RegisterType::F => self.f = bit_twiddling_utils::reset_bit(self.f, bit),
            RegisterType::IR => self.ir = bit_twiddling_utils::reset_bit(self.ir, bit),
            RegisterType::IE => self.ie = bit_twiddling_utils::reset_bit(self.ie, bit),
            _ => panic!("Bitwise operation 'reset' not allowed on register: {:?}", register),
        }
    }

    fn is_register_bit_set(&self, register: RegisterType, bit: u8) -> bool {
        match register {
            RegisterType::A => bit_twiddling_utils::is_bit_set(self.a, bit),
            RegisterType::F => bit_twiddling_utils::is_bit_set(self.f, bit),
            RegisterType::IR => bit_twiddling_utils::is_bit_set(self.ir, bit),
            RegisterType::IE => bit_twiddling_utils::is_bit_set(self.ie, bit),
            _ => panic!("Bitwise check 'is_set' not allowed on register: {:?}", register),
        }
    }

    pub fn set_flag(&mut self, flag: Flag) {
        self.set_register_bit(RegisterType::F, flag as u8);
    }

    pub fn reset_flag(&mut self, flag: Flag) {
        self.reset_register_bit(RegisterType::F, flag as u8);
    }

    pub fn is_flag_set(&self, flag: Flag) -> bool {
        self.is_register_bit_set(RegisterType::F, flag as u8)
    }

    pub fn enable_interrupt(&mut self, interrupt: Interrupt) {
        self.set_register_bit(RegisterType::IE, interrupt as u8);
    }

    pub fn disable_interrupt(&mut self, interrupt: Interrupt) {
        self.reset_register_bit(RegisterType::IE, interrupt as u8);
    }

    pub fn is_interrupt_enabled(&self, interrupt: Interrupt) -> bool {
        self.is_register_bit_set(RegisterType::IE, interrupt as u8)
    }
}










#[derive(Clone)]
struct Memory {
    rom_bank_0:                 [u8; 0x4000],   // 16KB
    switchable_rom_bank:        [u8; 0x4000],   // 16KB
    vram:                       [u8; 0x2000],   // 8KB
    external_ram:               [u8; 0x2000],   // 8KB
    wram:                       [u8; 0x2000],   // 8KB Work RAM (including both banks)
    oam:                        [u8; 0xA0],     // Sprite attribute table
    io_registers:               [u8; 0x80],     // I/O Registers
    hram:                       [u8; 0x7F],     // High RAM
    interrupt_enable_register:   u8,
}

impl Memory {
    fn new() -> Memory {
        Memory {
            rom_bank_0:                 [0; 0x4000],
            switchable_rom_bank:        [0; 0x4000],
            vram:                       [0; 0x2000],
            external_ram:               [0; 0x2000],
            wram:                       [0; 0x2000],
            oam:                        [0; 0xA0],
            io_registers:               [0; 0x80],
            hram:                       [0; 0x7F],
            interrupt_enable_register:   0,
        }
    }
    
    fn read(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x3FFF => self.rom_bank_0[address as usize],
            0x4000..=0x7FFF => self.switchable_rom_bank[address as usize - 0x4000],
            0x8000..=0x9FFF => self.vram[address as usize - 0x8000],
            0xA000..=0xBFFF => self.external_ram[address as usize - 0xA000],
            0xC000..=0xDFFF => self.wram[address as usize - 0xC000],
            0xFE00..=0xFE9F => self.oam[address as usize - 0xFE00],
            0xFF00..=0xFF7F => self.io_registers[address as usize - 0xFF00],
            0xFF80..=0xFFFE => self.hram[address as usize - 0xFF80],
            0xFFFF          => self.interrupt_enable_register,
            _               => 0,  // Handle unused memory areas and echo RAM
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x3FFF => (), // ROM is read-only
            0x4000..=0x7FFF => (), // Likewise, typically used for bank switching.
            0x8000..=0x9FFF => self.vram[address as usize - 0x8000] = value,
            0xA000..=0xBFFF => self.external_ram[address as usize - 0xA000] = value,
            0xC000..=0xDFFF => self.wram[address as usize - 0xC000] = value,
            0xFE00..=0xFE9F => self.oam[address as usize - 0xFE00] = value,
            0xFF00..=0xFF7F => self.io_registers[address as usize - 0xFF00] = value,
            0xFF80..=0xFFFE => self.hram[address as usize - 0xFF80] = value,
            0xFFFF          => self.interrupt_enable_register = value,
            _               => (), // Handle unused memory areas and echo RAM
        }
    }

    fn load_rom(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x3FFF => self.rom_bank_0[address as usize - 0x0000] = value,
            0x4000..=0x7FFF => self.switchable_rom_bank[address as usize - 0x4000] = value, 
            _               => (),
        }
    }

    fn dump(&self, range: (u16, u16)) {
        for address in range.0..=range.1 {
            println!("[{:#x}]: {:#x}", address, self.read(address));
        }
    }
}









#[derive(Clone)]
struct CPU {
    register_file: RegisterFile,
    memory: Memory,
}

impl CPU {
    fn new() -> Self {
        Self {
            register_file: RegisterFile::new(),
            memory: Memory::new(),
        }
    }

    fn log(&self, message: &str) {
        println!("{:<015} | PC: {:#^04X} | IR: {:#^02X} | mem[PC]: {:#X}", 
            message,
            self.register_file.read_register(&RegisterType::PC), 
            self.register_file.read_register(&RegisterType::IR), 
            self.memory.read(self.register_file.read_register(&RegisterType::PC))
        );
    }
}








/*--------------------------------------------
Things I could do:
    1. Work on boot ROM functionality
    2. Implement flag register
    3. Simple bank switching logic...if such a thing exists..?
    4. Create fetch_cycle routine 
--------------------------------------------*/
// GUI Code
fn main() {
    let log: bool = false;
    let mut cpu = CPU::new();

    let path = Path::new("/Users/thales/Documents/fragile-canvas/ROMs/DMG_ROM.bin");
    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", path.display(), why),
        Ok(file) => file,
    };

    let mut buffer = [0; 1];
    for address in 0x0000..=0x3FFF {
        if let Ok(bytes_read) = file.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            cpu.memory.load_rom(address, buffer[0]);
        }
    }
    
    let _current_instruction = cpu.memory.read(
        cpu.register_file.read_register(&RegisterType::PC));

    cpu.register_file.increment_reg(&RegisterType::PC);

    cpu.register_file.is_flag_set(Flag::HalfCarry);
    //cpu.memory.dump((0x0000, 0x0100));
    println!("{}", cpu.register_file);

    /* for _ in 0x00..=0x03 {
        //  CB-prefixed instructions
        if cpu.register_file.ir == 0xCB {
            cpu.register_file.pc = cpu.register_file.pc + 1;
            cpu.register_file.ir = cpu.memory.read(cpu.register_file.pc);


        }
        if cpu.register_file.ir == 0x31 {
            if log { cpu.log("LD SP, nn"); }
            
            let nn_lsb: u8 = cpu.memory.read(cpu.register_file.pc);
            cpu.register_file.pc = cpu.register_file.pc + 1;

            let nn_msb: u8 = cpu.memory.read(cpu.register_file.pc);
            cpu.register_file.pc = cpu.register_file.pc + 1;

            let nn: u16 = ((nn_msb as u16) << 8) | (nn_lsb as u16);
            cpu.register_file.sp = nn;
        }
        else if cpu.register_file.ir == 0xAF {
            if log {
                cpu.log("XOR A");
            }

            let temp: u8 = cpu.register_file.a ^ cpu.register_file.a;
            cpu.register_file.a = temp;

            cpu.register_file.set_flag(true, false, false, false);

            cpu.register_file.pc = cpu.register_file.pc + 1;
        }
        else if cpu.register_file.ir == 0x21 {
            if log {
                cpu.log("LD HL, $9fff");
            }

            let nn_lsb: u8 = cpu.memory.read(cpu.register_file.pc);
            cpu.register_file.pc = cpu.register_file.pc + 1;

            let nn_msb: u8 = cpu.memory.read(cpu.register_file.pc);
            cpu.register_file.pc = cpu.register_file.pc + 1;

            let nn: u16 = ((nn_msb as u16) << 8) | (nn_lsb as u16);
            cpu.register_file.set_hl(&nn);
        }
        else if cpu.register_file.ir == 0x32 {
            if log {
                cpu.log("LD (HL-), A");
            }

            cpu.memory.write(cpu.register_file.get_hl(), cpu.register_file.a);
            cpu.register_file.set_hl(&(cpu.register_file.get_hl() - 1));

            cpu.register_file.pc = cpu.register_file.pc + 1;
        }

        cpu.register_file.ir = cpu.memory.read(cpu.register_file.pc);
    } */
    //cpu.register_file.dump_core();
    //cpu.memory.dump((0x0000, 0x0103));
}








#[cfg(test)]
mod integration_tests {
    use super::*;
    use rand::Rng;
    use indicatif::ProgressBar;

    #[test]
    fn test_memory_register_integration() {
        let mut cpu = CPU::new();
        let mut rng = rand::thread_rng();

        let n: u64 = 1000000;
        let bar = ProgressBar::new(n);
        for _ in 0..=n {
            bar.inc(1);
            let bc_addr_data = rng.gen_range(0x8000..=0x9FFF); 
            let de_addr_data = rng.gen_range(0xA000..=0xBFFF);
            let hl_addr_data = rng.gen_range(0xC000..=0xDFFF);
            
            cpu.register_file.write_register(&RegisterType::BC, bc_addr_data);
            cpu.register_file.write_register(&RegisterType::DE, de_addr_data);
            cpu.register_file.write_register(&RegisterType::HL, hl_addr_data);

            let bc_memory_dummy = rng.gen_range(0x00..0xFF); 
            let de_memory_dummy = rng.gen_range(0x00..0xFF);
            let hl_memory_dummy = rng.gen_range(0x00..0xFF);

            cpu.memory.write(cpu.register_file.read_register(&RegisterType::BC), bc_memory_dummy);
            cpu.memory.write(cpu.register_file.read_register(&RegisterType::DE), de_memory_dummy);
            cpu.memory.write(cpu.register_file.read_register(&RegisterType::HL), hl_memory_dummy);
            
            assert_eq!(cpu.memory.read(bc_addr_data), bc_memory_dummy, "Register BC failed integration test...");
            assert_eq!(cpu.memory.read(de_addr_data), de_memory_dummy, "Register DE failed integration test...");
            assert_eq!(cpu.memory.read(hl_addr_data), hl_memory_dummy, "Register HL failed integration test...");
        }
        bar.finish();

        /*
        Filter/test fails if two random addresses lie within the same invalid memory space...
        */
        fn valid_mem_filter(addr0: &u8, addr1: &u8) -> (u8, u8)  {
            let addr = ((*addr0 as u16) << 8) | (*addr1 as u16);

            match addr {
                0x0000..=0x3FFF => (0xFF, 0xFF), // ROM is read-only.
                0x4000..=0x7FFF => (0xFF, 0xFE), // likewise
                0x8000..=0x9FFF => (*addr0, *addr1),
                0xA000..=0xBFFF => (*addr0, *addr1),
                0xC000..=0xDFFF => (*addr0, *addr1),
                0xFE00..=0xFE9F => (*addr0, *addr1),
                0xFF00..=0xFF7F => (*addr0, *addr1),
                0xFF80..=0xFFFE => (*addr0, *addr1),
                0xFFFF          => (*addr0, *addr1),
                _               => (0xFF, 0xFD), // handle unused memory areas and echo RAM
            }
        }

        let b_data = rng.gen_range(0x00..0xFF);
        let c_data = rng.gen_range(0x00..0xFF);

        let d_data = rng.gen_range(0x00..0xFF);
        let e_data = rng.gen_range(0x00..0xFF);

        let h_data = rng.gen_range(0x00..0xFF);
        let l_data = rng.gen_range(0x00..0xFF);

        let (filtered_b, filtered_c) = valid_mem_filter(&b_data, &c_data);
        let (filtered_d, filtered_e) = valid_mem_filter(&d_data, &e_data);
        let (filtered_h, filtered_l) = valid_mem_filter(&h_data, &l_data);

        cpu.register_file.b = filtered_b;
        cpu.register_file.c = filtered_c;

        cpu.register_file.d = filtered_d;
        cpu.register_file.e = filtered_e;

        cpu.register_file.h = filtered_h;
        cpu.register_file.l = filtered_l;

        let filtered_bc = ((filtered_b as u16) << 8) | (filtered_c as u16);
        let filtered_de = ((filtered_d as u16) << 8) | (filtered_e as u16);
        let filtered_hl = ((filtered_h as u16) << 8) | (filtered_l as u16);

        assert_eq!(cpu.register_file.read_register(&RegisterType::BC), filtered_bc, "Register BC failed write testing...");
        assert_eq!(cpu.register_file.read_register(&RegisterType::DE), filtered_de, "Register DE failed write testing...");
        assert_eq!(cpu.register_file.read_register(&RegisterType::HL), filtered_hl, "Register HL failed write testing...");

        let bc_memory_dummy0 = rng.gen_range(0x00..0xFF); 
        let de_memory_dummy0 = rng.gen_range(0x00..0xFF);
        let hl_memory_dummy0 = rng.gen_range(0x00..0xFF);

        cpu.memory.write(cpu.register_file.read_register(&RegisterType::BC), bc_memory_dummy0);
        cpu.memory.write(cpu.register_file.read_register(&RegisterType::DE), de_memory_dummy0);
        cpu.memory.write(cpu.register_file.read_register(&RegisterType::HL), hl_memory_dummy0);

        println!("b+c_test | memory dump [{:#X}]: {:#X}", cpu.register_file.read_register(&RegisterType::BC), cpu.memory.read(filtered_bc));
        println!("d+e_test | memory dump [{:#X}]: {:#X}", cpu.register_file.read_register(&RegisterType::DE), cpu.memory.read(filtered_de));
        println!("h+l_test | memory dump [{:#X}]: {:#X}", cpu.register_file.read_register(&RegisterType::HL), cpu.memory.read(filtered_hl));

        assert_eq!(cpu.memory.read(filtered_bc), bc_memory_dummy0, "Register B+C failed integration test...");
        assert_eq!(cpu.memory.read(filtered_de), de_memory_dummy0, "Register D+E failed integration test...");
        assert_eq!(cpu.memory.read(filtered_hl), hl_memory_dummy0, "Register H+L failed integration test...");
    }
}


