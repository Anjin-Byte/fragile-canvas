## 🧠 **Core Logic Primitives**

### 🗂️ **Register and Memory Access (10)**
1. `read_reg8(r)` – Read 8-bit register  
2. `write_reg8(r, value)` – Write 8-bit register  
3. `read_reg16(rr)` – Read 16-bit register  
4. `write_reg16(rr, value)` – Write 16-bit register  
5. `read_memory(addr)` – Read from memory  
6. `write_memory(addr, value)` – Write to memory  
7. `read_immediate8()` – Read next byte from PC  
8. `read_immediate16()` – Read next two bytes from PC (little-endian)  
9. `calc_addr_high_page(offset)` – Compute address `0xFF00 + offset`  
10. `calc_addr_indirect(rr)` – Get address from register pair  

---

### ➕ **Arithmetic & Logic (12)**
11. `add8(a, b)` – 8-bit addition with flags Z, H, C  
12. `adc8(a, b, carry_in)` – 8-bit add with carry  
13. `sub8(a, b)` – 8-bit subtraction with flags Z, H, C  
14. `sbc8(a, b, carry_in)` – 8-bit subtract with carry  
15. `cp8(a, b)` – Compare A - b without changing A  
16. `and8(a, b)` – Bitwise AND  
17. `or8(a, b)` – Bitwise OR  
18. `xor8(a, b)` – Bitwise XOR  
19. `cpl8(a)` – One's complement  
20. `inc8(a)` – Increment with flag update (Z, H)  
21. `dec8(a)` – Decrement with flag update (Z, H)  
22. `daa(a, flags)` – Decimal adjust accumulator using flags

---

### 🔢 **16-bit Arithmetic (6)**
23. `add16(a16, b16)` – 16-bit add with carry/half-carry for bit 11, 15  
24. `add_sp_e(sp, e)` – Add signed 8-bit to SP, update flags (H, C)  
25. `inc16(rr)` – Increment 16-bit value  
26. `dec16(rr)` – Decrement 16-bit value  
27. `push16(value)` – Push 16-bit to stack (two `write_memory`)  
28. `pop16()` – Pop 16-bit from stack (two `read_memory`)  

---

### 🔁 **Bit and Rotate Ops (12)**
29. `rlc(val)` – Rotate left, bit7 → bit0, C = bit7  
30. `rl(val, carry_in)` – Rotate left through carry  
31. `rrc(val)` – Rotate right, bit0 → bit7, C = bit0  
32. `rr(val, carry_in)` – Rotate right through carry  
33. `sla(val)` – Shift left, LSB=0, C=bit7  
34. `sra(val)` – Shift right, MSB unchanged, C=bit0  
35. `srl(val)` – Shift right, MSB=0, C=bit0  
36. `swap_nibbles(val)` – Swap upper/lower nibbles  
37. `bit_test(val, bit)` – Test bit, set Z based on result  
38. `set_bit(val, bit)` – Set bit  
39. `reset_bit(val, bit)` – Clear bit  
40. `mask_flags(value)` – Used for POP AF to mask lower nibble  

---

### 🧭 **Control Flow (9)**
41. `check_condition(cc, flags)` – Evaluate NZ/Z/NC/C  
42. `jump_abs(addr)` – Set PC to absolute address  
43. `jump_rel(offset)` – Add signed offset to PC  
44. `call(addr)` – Push current PC, set PC to addr  
45. `ret()` – Pop PC from stack  
46. `reti()` – Pop PC from stack, then IME = 1  
47. `rst(addr)` – Push PC, jump to fixed address  
48. `fetch_opcode()` – Fetch next opcode byte  
49. `decode_cb(opcode)` – Decode CB-prefixed op  

---

### ⚙️ **System/Interrupt State (7)**
50. `set_flag(name, val)` – Set specific flag (Z/N/H/C)  
51. `get_flag(name)` – Get current flag value  
52. `set_ime(value)` – Set IME (global interrupt enable)  
53. `defer_ime_enable()` – Schedule IME=1 after next instr  
54. `trigger_halt()` – Enter halt state  
55. `trigger_stop()` – Enter stop state  
56. `check_interrupts()` – Check and service pending interrupts

---

## ✅ **Total Unique Micro-ops (Primitives): 56**

This count includes **every unique micro-op** necessary to express the SM83’s entire ISA, accounting for instruction decomposition, flag logic, state transitions, and bitwise manipulation. These primitives are **sufficient and minimal** for building a faithful emulator or formal model of the SM83.

Would you like me to generate a visual dependency map between instructions and these primitives?