use cpu::CPU;
use memory::mmu::MMU;
use std::{cell::RefCell, fs::File, io::Read, path::Path, rc::Rc};

pub mod cpu;
pub mod memory;
pub mod utils;

fn main() {
    let mut mmu = MMU::new();

    let path = Path::new("../ROMs/DMG_ROM.bin");
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
            mmu.load_rom(address, buffer[0]);
        }
    }

    let shared_bus = Rc::new(RefCell::new(mmu));
    let mut cpu = CPU::new(shared_bus);
    for _ in 0..3 {
        cpu.tick();
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::cpu::registers::Reg16;

    use super::*;
    use indicatif::ProgressBar;
    use rand::Rng;

    #[test]
    fn test_memory_register_integration() {
        let mmu = MMU::new();
        let shared_bus = Rc::new(RefCell::new(mmu));
        let mut cpu = CPU::new(shared_bus);
        let mut rng = rand::thread_rng();

        let n: u64 = 1000000;
        let bar = ProgressBar::new(n);
        for _ in 0..=n {
            bar.inc(1);
            let bc_addr_data = rng.gen_range(0x8000..=0x9FFF);
            let de_addr_data = rng.gen_range(0xA000..=0xBFFF);
            let hl_addr_data = rng.gen_range(0xC000..=0xDFFF);

            cpu.register_file.set_16bit(&Reg16::BC, bc_addr_data);
            cpu.register_file.set_16bit(&Reg16::DE, de_addr_data);
            cpu.register_file.set_16bit(&Reg16::HL, hl_addr_data);

            let bc_memory_dummy = rng.gen_range(0x00..0xFF);
            let de_memory_dummy = rng.gen_range(0x00..0xFF);
            let hl_memory_dummy = rng.gen_range(0x00..0xFF);

            cpu.bus
                .borrow_mut()
                .write(cpu.register_file.get_16bit(&Reg16::BC), bc_memory_dummy);
            cpu.bus
                .borrow_mut()
                .write(cpu.register_file.get_16bit(&Reg16::DE), de_memory_dummy);
            cpu.bus
                .borrow_mut()
                .write(cpu.register_file.get_16bit(&Reg16::HL), hl_memory_dummy);

            assert_eq!(
                cpu.bus.borrow_mut().read(bc_addr_data),
                bc_memory_dummy,
                "Register BC failed integration test..."
            );
            assert_eq!(
                cpu.bus.borrow_mut().read(de_addr_data),
                de_memory_dummy,
                "Register DE failed integration test..."
            );
            assert_eq!(
                cpu.bus.borrow_mut().read(hl_addr_data),
                hl_memory_dummy,
                "Register HL failed integration test..."
            );
        }
        bar.finish();

        /*
            Filter/test fails if two random addresses lie within the same invalid memory space...

            This test is broken... whoops :)
            *//*
            fn valid_mem_filter(addr0: &u8, addr1: &u8) -> (u8, u8) {
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
                    0xFFFF => (*addr0, *addr1),
                    _ => (0xFF, 0xFD), // handle unused memory areas and echo RAM
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

            cpu.register_file.set_8bit(&Reg8::B, filtered_b);
            cpu.register_file.set_8bit(&Reg8::C, filtered_c);

            cpu.register_file.set_8bit(&Reg8::D, filtered_d);
            cpu.register_file.set_8bit(&Reg8::E, filtered_e);

            cpu.register_file.set_8bit(&Reg8::H, filtered_h);
            cpu.register_file.set_8bit(&Reg8::L, filtered_l);

            let filtered_bc = ((filtered_b as u16) << 8) | (filtered_c as u16);
            let filtered_de = ((filtered_d as u16) << 8) | (filtered_e as u16);
            let filtered_hl = ((filtered_h as u16) << 8) | (filtered_l as u16);

            assert_eq!(
                cpu.register_file.get_16bit(&Reg16::BC),
                filtered_bc,
                "Register BC failed write testing..."
            );
            assert_eq!(
                cpu.register_file.get_16bit(&Reg16::DE),
                filtered_de,
                "Register DE failed write testing..."
            );
            assert_eq!(
                cpu.register_file.get_16bit(&Reg16::HL),
                filtered_hl,
                "Register HL failed write testing..."
            );

            let bc_memory_dummy0 = rng.gen_range(0x00..0xFF);
            let de_memory_dummy0 = rng.gen_range(0x00..0xFF);
            let hl_memory_dummy0 = rng.gen_range(0x00..0xFF);

            cpu.bus.borrow_mut().write(
                cpu.register_file.get_16bit(&Reg16::BC),
                bc_memory_dummy0,
            );
            cpu.bus.borrow_mut().write(
                cpu.register_file.get_16bit(&Reg16::DE),
                de_memory_dummy0,
            );
            cpu.bus.borrow_mut().write(
                cpu.register_file.get_16bit(&Reg16::HL),
                hl_memory_dummy0,
            );

            println!(
                "b+c_test | memory dump [{:#X}]: {:#X}",
                cpu.register_file.get_16bit(&Reg16::BC),
                cpu.bus.borrow_mut().read(filtered_bc)
            );
            println!(
                "d+e_test | memory dump [{:#X}]: {:#X}",
                cpu.register_file.get_16bit(&Reg16::DE),
                cpu.bus.borrow_mut().read(filtered_de)
            );
            println!(
                "h+l_test | memory dump [{:#X}]: {:#X}",
                cpu.register_file.get_16bit(&Reg16::HL),
                cpu.bus.borrow_mut().read(filtered_hl)
            );

            assert_eq!(
                cpu.bus.borrow_mut().read(filtered_bc),
                bc_memory_dummy0,
                "Register B+C failed integration test..."
            );
            assert_eq!(
                cpu.bus.borrow_mut().read(filtered_de),
                de_memory_dummy0,
                "Register D+E failed integration test..."
            );
            assert_eq!(
                cpu.bus.borrow_mut().read(filtered_hl),
                hl_memory_dummy0,
                "Register H+L failed integration test..."
            );
        */
    }
}
