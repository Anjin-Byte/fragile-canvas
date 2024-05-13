use core::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

//use std::env;
use eframe::egui::{Color32, Label, ScrollArea};
use eframe::{egui::CentralPanel, epi::App};

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
        f.write_str("[")?;
        fmt::Display::fmt(&self.pc, f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.sp, f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.a, f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.get_bc(), f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.get_de(), f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.get_hl(), f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.ir, f)?;
        f.write_str(", ")?;
        fmt::Display::fmt(&self.ie, f)?;
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

    fn dump_core(&self) {
        println!("PC: {:#X} | SP: {:#X}", self.pc, self.sp);
        println!("BC: {:#X} | DE: {:#X} | HL: {:#X}", self.get_bc(), self.get_de(), self.get_hl());
        println!("A: {:#X} | F: {:#X} | IR: {:#X} | IE: {:#X}", self.a, self.f, self.ir, self.ie);
    }

    fn get_bc(&self) -> u16 {
        ((self.b as u16) << 8) | (self.c as u16)
    }

    fn set_bc(&mut self, data: &u16) {
        self.b = ((data >> 8) & 0xFF) as u8;
        self.c = (data & 0xFF) as u8;
    }

    fn get_de(&self) -> u16 {
        ((self.d as u16) << 8) | (self.e as u16)
    }

    fn set_de(&mut self, data: &u16) {
        self.d = ((data >> 8) & 0xFF) as u8;
        self.e = (data & 0xFF) as u8;
    }    
    
    fn get_hl(&self) -> u16 {
        ((self.h as u16) << 8) | (self.l as u16)
    }

    fn set_hl(&mut self, data: &u16) {
        self.h = ((data >> 8) & 0xFF) as u8;
        self.l = (data & 0xFF) as u8;
    }

    fn set_flag(&mut self, z: bool, n: bool, h: bool, c:bool) {
        if z {
            self.f |= 0x80;
        } 
        else {
            self.f &= !0x80;
        }


        if n {
            self.f |= 0x40;
        } 
        else {
            self.f &= !0x40;
        }


        if h {
            self.f |= 0x20;
        } 
        else {
            self.f &= !0x20;
        }


        if c {
            self.f |= 0x10;
        } 
        else {
            self.f &= !0x10;
        }
    }

    fn _reset_flag(&mut self, z: bool, n: bool, h: bool, c:bool ) {
        if z {
            self.f &= !0x80;
        }

        if n {
            self.f &= !0x40;
        }

        if h {
            self.f &= !0x20;
        }

        if c {
            self.f &= !0x10;
        }
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

    fn _dump(&self, range: (u16, u16)) {
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
            self.register_file.pc, 
            self.register_file.ir, 
            self.memory.read(self.register_file.pc)
        );
    }

    fn render_cards(&self, ui: &mut eframe::egui::Ui) {
   
        const PADDING: f32 = 5.0;
        ui.add_space(PADDING);

        let title = format!(" BLAHHH");
        const WHITE: Color32 = Color32::from_rgb(255, 255, 255);
        ui.colored_label(WHITE, title);

        ui.add_space(PADDING);
        let desc = Label::new(self.register_file.clone()).text_style(eframe::egui::TextStyle::Button);
        ui.add(desc);

            
        
    }
}

impl App for CPU {
    fn setup(
        &mut self,
        _ctx: &eframe::egui::CtxRef,
        _frame: &mut eframe::epi::Frame<'_>,
        _storage: Option<&dyn eframe::epi::Storage>,
    ) {
        
    }

    fn update(
        &mut self, 
        ctx: &eframe::egui::CtxRef, 
        _frame: &mut eframe::epi::Frame<'_>) {
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::auto_sized().show(ui, |ui| {
                self.render_cards(ui);
            });
        });
    }

    fn name(&self) -> &str {
        "headlines"
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
    //let args: Vec<String> = env::args().collect();
    let log: bool = false;
    //if args[1] == "l" {
        //log = true
    //}

    
    let mut cpu = CPU::new();
    //let mut native_options = NativeOptions::default();
    //native_options.initial_window_size = Some(Vec2::new(540., 960.));
    //run_native(Box::new(cpu.clone()), native_options);


    let path = Path::new("/home/thale/scratch/DMG_ROM.bin");
    //let path = Path::new("/home/thale/scratch/Tetris (USA) (Rev-A).gb");
    let display = path.display();
    // println!("{}", display);

    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, why),
        Ok(file) => file,
    };

    let mut buffer = [0; 1];
    for address in 0x0000..=0x3FFF {
        if let Ok(bytes_read) = file.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            // println!("{:#X}", buffer[0]);
            cpu.memory.load_rom(address, buffer[0]);
        }
    }

    cpu.register_file.ir = cpu.memory.read(cpu.register_file.pc);
    cpu.register_file.pc = cpu.register_file.pc + 1;
    
    for _ in 0x00..=0x03 {
        //  CB-prefixed instructions
        if cpu.register_file.ir == 0xCB {
            cpu.register_file.pc = cpu.register_file.pc + 1;
            cpu.register_file.ir = cpu.memory.read(cpu.register_file.pc);


        }
        if cpu.register_file.ir == 0x31 {
            if log {
                cpu.log("LD SP, nn");
            }
            
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
    }
    cpu.register_file.dump_core();
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

            // println!("bc_addr_data: {:#X}", bc_addr_data);
            // println!("de_addr_data: {:#X}", de_addr_data);
            // println!("hl_addr_data: {:#X}", hl_addr_data);

            cpu.register_file.set_bc(&bc_addr_data);
            cpu.register_file.set_de(&de_addr_data);
            cpu.register_file.set_hl(&hl_addr_data);

            // println!("bc_reg_dump: {:#X}", cpu.register_file.get_bc());
            // println!("de_reg_dump: {:#X}", cpu.register_file.get_de());
            // println!("hl_reg_dump: {:#X}", cpu.register_file.get_hl());

            let bc_memory_dummy = rng.gen_range(0x00..0xFF); 
            let de_memory_dummy = rng.gen_range(0x00..0xFF);
            let hl_memory_dummy = rng.gen_range(0x00..0xFF);

            // println!("bc_memory_dummy: {:#X}", bc_memory_dummy);
            // println!("de_memory_dummy: {:#X}", de_memory_dummy);
            // println!("hl_memory_dummy: {:#X}", hl_memory_dummy);

            cpu.memory.write(cpu.register_file.get_bc(), bc_memory_dummy);
            cpu.memory.write(cpu.register_file.get_de(), de_memory_dummy);
            cpu.memory.write(cpu.register_file.get_hl(), hl_memory_dummy);

            // println!("bc_test | memory dump [{:#X}]: {:#X}", cpu.register_file.get_bc(), cpu.memory.read(bc_addr_data));
            // println!("de_test | memory dump [{:#X}]: {:#X}", cpu.register_file.get_de(), cpu.memory.read(de_addr_data));
            // println!("hl_test | memory dump [{:#X}]: {:#X}", cpu.register_file.get_hl(), cpu.memory.read(hl_addr_data));
            
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

        assert_eq!(cpu.register_file.get_bc(), filtered_bc, "Register BC failed write testing...");
        assert_eq!(cpu.register_file.get_de(), filtered_de, "Register DE failed write testing...");
        assert_eq!(cpu.register_file.get_hl(), filtered_hl, "Register HL failed write testing...");

        let bc_memory_dummy0 = rng.gen_range(0x00..0xFF); 
        let de_memory_dummy0 = rng.gen_range(0x00..0xFF);
        let hl_memory_dummy0 = rng.gen_range(0x00..0xFF);

        cpu.memory.write(cpu.register_file.get_bc(), bc_memory_dummy0);
        cpu.memory.write(cpu.register_file.get_de(), de_memory_dummy0);
        cpu.memory.write(cpu.register_file.get_hl(), hl_memory_dummy0);

        println!("b+c_test | memory dump [{:#X}]: {:#X}", cpu.register_file.get_bc(), cpu.memory.read(filtered_bc));
        println!("d+e_test | memory dump [{:#X}]: {:#X}", cpu.register_file.get_de(), cpu.memory.read(filtered_de));
        println!("h+l_test | memory dump [{:#X}]: {:#X}", cpu.register_file.get_hl(), cpu.memory.read(filtered_hl));

        assert_eq!(cpu.memory.read(filtered_bc), bc_memory_dummy0, "Register B+C failed integration test...");
        assert_eq!(cpu.memory.read(filtered_de), de_memory_dummy0, "Register D+E failed integration test...");
        assert_eq!(cpu.memory.read(filtered_hl), hl_memory_dummy0, "Register H+L failed integration test...");
    }
}


