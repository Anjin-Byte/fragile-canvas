# Reverse-Engineering the Game Boy’s SM83 ALU – From Transistors to Instructions

## Introduction  
The Game Boy’s CPU, a Sharp SM83 core (often called LR35902), is an 8-bit microprocessor with a built-in Arithmetic Logic Unit (ALU) that handles all arithmetic and logical operations. By reverse-engineering this ALU from the lowest levels (transistor logic and gate design) up through the microarchitecture and finally to the visible instruction set, we can understand how hardware design decisions enabled (and constrained) the Game Boy’s instruction set features. In particular, we will see how the **4-bit slice design** of the ALU, inherited from its Intel 8080/Z80 lineage, influences the handling of CPU flags and certain quirky instruction behaviors. We will also examine the interplay between the ALU and the flag register (F) – which contains Zero (Z), Subtract (N), Half Carry (H), and Carry (C) flags – explaining exactly which instructions affect which flags and why. Throughout, diagrams and tables will help connect the dots between the ALU’s gate-level design, the microarchitectural control logic, and the Game Boy’s ISA, shedding light on outliers like decimal adjust instructions and unusual flag behaviors.

## ALU Hardware Design: 4-Bit Slice Architecture  
At the hardware level, the SM83’s ALU is implemented as a **4-bit-wide computation unit** that processes an 8-bit operation in two steps (nibbles) rather than in one 8-bit chunk. This is a key design choice inherited from the Intel 8080/Z80 family: while many 8-bit CPUs of the era (like MOS 6502 or Intel 8085) used full 8-bit ALUs, the Z80/SM83 uses a *4-bit ALU core* ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20Z,advanced%20than%20the%208085%27s%20decimal)). In practice, this means the ALU computes the lower 4 bits of a result first, then the upper 4 bits, using the carry-out from the low nibble in the high nibble’s calculation ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20two%20operands%20go%20to,the%20second%20computation%20if%20needed)) ([
The Nintendo® Game Boy™, Part 1: The Intel 8080 and the Zilog Z80. | RealBoy	](https://realboyemulator.wordpress.com/2013/01/01/the-nintendo-game-boy-1/#:~:text=second%20thing%20I%20learned%20is,complicated%20than%20what%20I%20first)). The benefit of this design is a reduction in transistor count and power – only a half-width adder/logic circuit is needed, reused twice per operation – at the cost of a more complex execution timing. The Game Boy’s 4.19 MHz clock is organized so that a single 8-bit ALU operation spans two of the internal 1 MHz machine cycles, with the ALU handling 4 bits per cycle.

 ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html)) *Figure: Block diagram of the Z80/SM83 ALU’s internal structure (registers and data paths are 4 bits wide). The ALU uses twin 4-bit “slices” with latches for the low and high nibbles. In the first cycle it operates on the low 4-bit operands (op1/op2 low latches) and stores the partial result in a latch; in the second cycle it operates on the high 4 bits (op1/op2 high latches) using the previously latched low-result and carry-out from the first step ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20two%20operands%20go%20to,the%20second%20computation%20if%20needed)).*  

Internally, each 1-bit slice of the ALU consists of logic that can compute multiple functions: sum (with carry-in), bitwise AND, OR, XOR, etc., based on control signals. Reverse-engineered transistor schematics from the Z80 confirm that each bit-slice has a multi-level gate network to produce these operations ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=,so%20the%20lower%20AND%20gate)). For example, one combination of control lines will enable full addition (summing the two input bits and a carry-in), while other combinations effectively force the carry inputs to 0 or 1 to yield logical AND, OR or XOR results instead ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=,in%29%20is)). A dedicated inverter on one operand allows the same adder circuit to perform subtraction: for a subtraction, the second operand is fed in inverted (two’s complement), and the carry-in is preset (effectively adding 1) ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=op2%20latches%20are%20connected%20to,for%20subtraction%2C%20negation%2C%20and%20comparison)). This is how instructions like SUB or CP (compare) are realized by the same hardware as addition – the hardware doesn’t “subtract” in a distinct way; it just adds the twos-complement of the operand.

Crucially, after the low-nibble computation, the partial 4-bit result is stored in a latch, and the carry-out from bit 3 (the half-byte) is held. Then the ALU processes the high nibble: it takes the carry from the low half as an input carry, computes the high 4-bit result, and finally writes the combined 8-bit result back to the internal bus ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20two%20operands%20go%20to,the%20second%20computation%20if%20needed)). By doing the work in two steps, a “half-carry” from bit 3→4 naturally occurs if the low nibble addition overflowed – and this directly informs the CPU’s Half Carry flag (more on that in a moment). The ALU’s two-step sequence is usually invisible to the programmer because it is tightly coupled to the CPU’s timing. In fact, the Game Boy’s CPU pipelines this process: the second 4-bit ALU cycle overlaps with the fetch of the next instruction, so simple instructions still complete in one machine cycle despite the two micro-operations ([
The Nintendo® Game Boy™, Part 1: The Intel 8080 and the Zilog Z80. | RealBoy	](https://realboyemulator.wordpress.com/2013/01/01/the-nintendo-game-boy-1/#:~:text=second%20thing%20I%20learned%20is,complicated%20than%20what%20I%20first)). In summary, the SM83 ALU’s physical design is a *serial 4-bit ALU* that produces an 8-bit result with a carry-lookahead between the nibbles. This design efficiently implements all 8-bit arithmetic/logic ops and even 16-bit arithmetic (by chaining two 8-bit operations) while saving die area.

## Microarchitecture and Control Integration  
The ALU in the SM83 does not exist in isolation – it’s part of a broader microarchitecture that includes the register file, instruction decode logic, and internal data buses. The Game Boy CPU uses an **accumulator-based design**: one of the ALU’s input operands is usually the accumulator register A (the primary 8-bit register), and the other operand comes from either another register, an immediate value, or memory. A simplified schematic of the LR35902/SM83 microarchitecture shows that the A register is wired into one side of the ALU (often labeled the “ACC” input), while a temporary register or the general-purpose registers supply the second ALU operand via an internal bus and multiplexers ([
    
    FPGA Game Boy Part 3: ALU and some microcode · Craig J. Bishop
    
  ](https://craigjb.com/2018/04/13/alu-microcode/#:~:text=If%20we%20look%20again%20at,operations%20update%20all%20flag%20bits)) ([
    
    FPGA Game Boy Part 3: ALU and some microcode · Craig J. Bishop
    
  ](https://craigjb.com/2018/04/13/alu-microcode/#:~:text=operand%20to%20the%20ALU%20comes,operations%20update%20all%20flag%20bits)). After the ALU computes a result, that result can be latched into a target register or written to memory through the internal bus. The 8-bit F register (flags) is updated with status bits as needed.

 ([
    
    FPGA Game Boy Part 3: ALU and some microcode · Craig J. Bishop
    
  ](https://craigjb.com/2018/04/13/alu-microcode/)) *Figure: Internal architecture of the Game Boy’s LR35902 (SM83) CPU, highlighting the ALU (red) and its connections ([
    
    FPGA Game Boy Part 3: ALU and some microcode · Craig J. Bishop
    
  ](https://craigjb.com/2018/04/13/alu-microcode/#:~:text=If%20we%20look%20again%20at,operations%20update%20all%20flag%20bits)). The ALU takes input from the Accumulator A and a second operand (via a TEMP register and multiplexer from the register file or memory), and outputs the result back into registers. A separate 16-bit incrementer (orange “+1”) handles PC, SP, and HL increments without using the main ALU ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=The%20SM83%20CPU%20core%20used,Basically)).*  

The **control logic** orchestrates multi-cycle operations and flag updates. Because the ALU is only 4 bits wide internally, most 8-bit instructions involve two sub-operations, as described earlier. The control unit (sequencer) ensures that for an 8-bit addition, the ALU is fed first the low 4-bit operands, then the high 4-bit operands, and that the carry between them is propagated. In terms of timing, the Game Boy divides each instruction into CPU cycles (each 4 system clocks). A simple 8-bit ALU operation like `ADD A,B` fits into a single 4-clock cycle: the first half of that cycle does operand fetch and low-nibble ALU compute, and the second half does the high-nibble compute and write-back, overlapping with the next fetch ([
The Nintendo® Game Boy™, Part 1: The Intel 8080 and the Zilog Z80. | RealBoy	](https://realboyemulator.wordpress.com/2013/01/01/the-nintendo-game-boy-1/#:~:text=clock%20cycles,complicated%20than%20what%20I%20first)).

One interesting aspect of the SM83 (inherited from the Z80) is that it contains some **dedicated 16-bit arithmetic logic separate from the main ALU**. In particular, there is an increment/decrement unit for 16-bit addresses (used for the 16-bit registers HL, BC, DE, SP and the Program Counter) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=The%20SM83%20CPU%20core%20used,Basically)). This unit can increment or decrement a 16-bit value in one go and is used for instructions like INC BC, INC HL, etc., and for updating the PC and stack pointer on pushes/pops. However, this 16-bit incrementer does *not* affect the flag register ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=normal%20Z80%20CPU%2C%20it%20also,Basically)). Therefore, any 16-bit operation that needs to set flags (such as `ADD HL, DE` or the special `ADD SP, e` and `LD HL, SP+e` instructions) must go through the main 8-bit ALU in two steps (low byte then high byte) so that the half-carry and carry flags can be generated correctly ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=Basically%3A)) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=We%20don%27t%20yet%20have%20a,then%20load%20SP%20from%20it)). The control logic essentially microcodes these multi-byte operations. For example, a 16-bit add of HL + DE is internally executed as something like: “add L + E -> L (with internal carry); then add H + D + carry -> H”, which is why it takes two 4-clock cycles to complete and why the half-carry flag in that case reflects a carry out of bit 11 (the carry from the lower 12 bits) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). As another example, the instruction `ADD SP, e` (adding an 8-bit signed immediate to SP) is performed by the ALU in two 8-bit steps: low byte of SP + immediate low, then high byte + carry/overflow, with the result put back into SP. Interestingly, the CPU doesn’t update SP until both sub-operations are done; internally it computes the result in a hidden register then writes it to SP in one go ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=We%20don%27t%20yet%20have%20a,then%20load%20SP%20from%20it)). This is why the flags get updated for `ADD SP,e` even though SP is updated “atomically” at the end ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=,in%20the%20physical%20hardware%3F)).

To summarize the microarchitectural view: the ALU receives control signals from the decode logic indicating which operation to perform (add, adc, sub, and, or, xor, etc.), which often boils down to setting the ALU’s internal control lines (as noted earlier: e.g. invert second operand for subtract, force carry-in=0 for logical ops, etc.). One operand is typically the accumulator A (for most arithmetic/logic instructions), though for non-accumulator operations like `INC B` or `DEC C`, the CPU will route the operand through the same ALU by loading it into the A register or using an internal TEMP register as the ALU input. (The Game Boy’s design is simplified compared to a full Z80: it doesn’t have distinct IX/IY registers or a separate ALU for them; everything funnels through the one ALU and a limited set of internal holding registers.) The result from the ALU goes onto the internal bus and is clocked into the destination register. Meanwhile, dedicated **flag logic** monitors the ALU outputs (and inputs) to set or reset the Zero, Subtract, Half-Carry, and Carry flags appropriately, as we will detail next.

## ALU Operation and Flag Behavior  
The Game Boy CPU’s flag register (F) has four bits that are updated by ALU operations: Z (zero), N (add/subtract), H (half-carry), and C (carry). Each instruction that affects the flags does so in a specific way determined by the ALU’s behavior during that operation. We can categorize how the ALU interacts with the flags for various types of instructions:

- **8-Bit Addition (ADD/ADC)**: These instructions add an 8-bit value to the accumulator A (with ADC including the prior carry). The ALU performs a full 8-bit addition in two halves, so the flags are set as follows: **Z** is set if the 8-bit result is zero; **N** is reset (0) because the operation is addition (not a subtraction); **H** is set if there was a carry from bit 3 to bit 4 (i.e. if the low nibble addition overflowed) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)); and **C** is set if there was a carry from bit 7 (i.e. if the full 8-bit result overflowed past 0xFF) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). For example, adding 0x3F and 0x42: the low nibble F + 2 causes a carry into the high nibble, so H=1, and the high nibble addition also carries out (3 + 4 + carry = 8 with carry), so C=1. The Zero flag would be 0 in that case (result 0x81 is not zero). These rules apply to `ADD A,n` and `ADC A,n` (with ADC just adding the extra carry-in). They also apply to increments (`INC r`), which are effectively addition of 1: INC sets N=0, and sets H if the low 4 bits of the register went from 0xF to 0x0 (carry out of bit3) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)). INC does not affect C (since it’s not a full addition with carry in) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)).

- **8-Bit Subtraction (SUB/SBC/CP)**: Subtraction in two’s complement logic is done via addition of the inverted operand. The ALU’s carry-out then has an interpretation as “no borrow” or “borrow” because of the inversion. By convention, the flags are set so that **N = 1** (since these are subtraction operations) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,Set%20if%20A%20%3D%20n)). **Z** is set if the result of the subtraction is 0 (i.e. if A == operand for CP, or after SUB) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,Set%20if%20A%20%3D%20n)). **H** is set if there was *no* borrow from bit 4, which is equivalent to saying the borrow (if any) didn’t need to propagate past the low nibble. In practical terms, for subtraction `A - n`, H=1 if `(A & 0xF) >= (n & 0xF)` (the lower nibble of A was sufficient, no borrow from bit4) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). If A’s low nibble was smaller than the operand’s low nibble, the ALU had to borrow from the high nibble, and H will be 0. **C** (carry flag) is set if no borrow occurred in the full 8-bit subtraction, i.e. if A >= n. If the subtraction needed an overall borrow (A < n), then C is cleared ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). This “invert carry for subtraction” behavior comes from the adder hardware: the ALU’s final carry-out bit is 1 in the case of a no-borrow subtraction. For example, if A=0x30 and you SUB 0x10, there’s no borrow; the result 0x20 yields C=1 (no borrow), H=1 (0x0 minus 0x0 nibble, no borrow at nibble), N=1, Z=0. But  if A=0x30 and you SUB 0x40, the result wraps around (0xF0 with borrow) – here C=0 (borrow occurred, since 0x30 < 0x40), H would also be 0 (because 0x0 < 0x0 with borrow from bit4 as well), N=1, Z=0. The compare instruction `CP n` behaves like a SUB that throws away the result: it affects flags (Z, N, H, C) just as a SUB would, without altering A ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,Set%20if%20A%20%3D%20n)).

- **16-Bit Addition (ADD HL, rr)**: The Game Boy CPU supports adding a 16-bit register (BC, DE, HL, or SP) into HL. This operation internally goes through the ALU twice (for low and high bytes), and the flag outcomes reflect the high-byte result. The **N flag is cleared (0)** because it’s an addition ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)), and **Z is not affected at all** (HL additions leave Z flag as whatever it was) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)). The Half Carry flag in this case is set if there was a carry from bit 11 (the lower half of the 16-bit value) into bit 12 ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). In effect, it’s indicating a carry from the low byte into the high byte during the 16-bit addition. The Carry flag is set if there was a carry from bit 15 (i.e. if the 16-bit result overflowed past 0xFFFF) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=H%20,from%20bit%2011)). So, for example, adding 0x0FFF + 0x0001 will set H (because carry from bit11 occurred when 0xF + 0x0 caused carry into bit12) and will not set C (no overflow past 0xFFFF). On the other hand, adding 0x8000 + 0x8000 will clear H (no carry from bit11, since low bytes 0x00+0x00) but set C (carry out of bit15 occurred). Note that after `ADD HL,rr`, the Zero flag is unchanged – an interesting quirk because 8-bit ADD did set Z. This is by design: on Z80/SM83, 16-bit additions don’t touch Z since they are often used for address math, not usually for conditional logic. Similarly, the 16-bit increment instructions (`INC nn` and `DEC nn`) do *not* affect any flags ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=DEC%20nn%20%20%20,Decrement%20register%20nn)) (since they use the dedicated incrementer unit, as mentioned). If a 16-bit result’s zero-ness or other condition needed to be checked, software would have to examine the result explicitly.

- **Add with Immediate to SP (`ADD SP, e` and `LD HL, SP+e`)**: The Game Boy introduced special instructions to assist with stack pointer arithmetic (useful for stack-relative addressing). `ADD SP, e` adds an 8-bit signed value to SP. It behaves similarly to a 16-bit add in terms of flags, **but only the lower byte’s half-carry and carry are reported**. In fact, the official behavior is: Z is cleared to 0, N is cleared to 0, **H is set if there’s a carry from bit 3 to 4 in the low byte addition**, and **C is set if there’s a carry from bit 7 to 8 (i.e. an overflow out of the low byte) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=TL%3BDR%3A%20For%20,bit%203%20to%20bit%204)) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=I%20ran%20this%20on%203,12%20did%20not))**. Essentially, it’s treating the operation as an 8-bit add for flag purposes (just on the low byte of SP). Confirming this, tests on real hardware show that adding an offset to SP only triggers the H flag on a nibble carry in the *low* byte, and ignores carries out of the high byte except for the final C flag ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=For%20each%20case%20I%20store,those%20bytes%20on%20the%20screen)). The instruction `LD HL, SP+e` performs the same computation, but stores the result in HL instead of SP. It has the *same flag behavior* as `ADD SP, e` – Z and N are cleared, H and C reflect the low-byte addition only ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)). This is a somewhat unusual instruction: even though it’s conceptually a 16-bit addition, it only cares about half-carry on the lower 8 bits. The rationale is that these are meant for address calculations where you typically only need to know if the low byte wrapped around (for adjusting page boundaries, etc.). The high-byte carry (C flag) does indicate if an overflow past 0xFF occurred in the low byte, which often means a cross-page carry in addresses. The Zero flag is forced 0 because the result will virtually never be zero (and if it were, it’s not particularly meaningful for SP arithmetic), and N is 0 because it’s an addition.

- **Logical Operations (AND, OR, XOR)**: These operations ignore the notion of carries entirely and treat each bit independently via logic gates. The flags are set to a fixed pattern as per Z80 convention. **Z** is set if the result is 0 (e.g. ANDing A with a value that clears all bits will set Z) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)). **N** is reset to 0 (these are not subtraction operations) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)). **H** is set to 1 for the `AND` instruction (this is a quirk inherited from Z80: the half-carry flag is meaningless for logical ops, but on Z80 the H flag was repurposed in some cases. By convention AND sets H=1, whereas OR and XOR reset H=0) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). On the Game Boy, `AND n` will thus always set H=1 ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)), while `OR n` and `XOR n` will set H=0 ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). **C** is reset to 0 for all logical ops (they do not produce an actual carry out) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). For OR and XOR: Z flag set if result zero, N=0, H=0, C=0. For AND: Z flag set if zero, N=0, H=1, C=0. The setting of H for AND is largely irrelevant to gameplay programming, except that it can affect DAA (Decimal Adjust) if one were to use it afterward – generally not done, so H’s value after OR/XOR/AND is mostly just a curiosity of the design.

- **Rotate and Shift Instructions**: The Game Boy CPU has a variety of rotate/shift operations, some that operate only on the accumulator (A) and others that operate on any register or memory via the CB-prefixed opcodes. In all cases, the **C flag is set to the bit that was shifted out** of the byte. For a left rotation/shift, the bit leaving the high bit (bit 7) goes into C; for a right shift/rotate, the bit leaving bit 0 goes into C ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). The **Z flag** is set if the *result* is zero *only* for the versions that operate on arbitrary registers (the CB-prefixed opcodes) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). Notably, the single-bit rotates `RLCA`, `RLA`, `RRCA`, `RRA` (which are instructions that rotate A without using the CB prefix) do **not** set the Z flag – on the Game Boy, after these accumulator rotates, Z is left unaffected (or effectively reset to 0 by most implementations) since A cannot be zeroed by a pure rotate (except A was 0 to begin with, in which case A stays 0 but on real hardware Z remains 0 rather than become 1) ([Gameboy Z80: How should RLCA and RRCA behave?](https://groups.google.com/g/comp.emulators.misc/c/aHU3k1O8T8k#:~:text=What%20I%20am%20wondering%20about%2C,Z80)) ([Gameboy Z80: How should RLCA and RRCA behave?](https://groups.google.com/g/comp.emulators.misc/c/aHU3k1O8T8k#:~:text=,does%27nt%20set%20Z%20after%20RRCA%2FRLCA)). In short, the “A rotates” are treated like 8080-era rotates (only affect C), whereas the generic rotates/shifts (RLC n, RL n, etc.) set Z if the result is zero. All rotates/shifts clear **N=0** and **H=0** (since these are not arithmetic operations in the sense of using the adder) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=SRL%20n%20%20%20,of%20n%20set%20to%200)). For example, `SLA B` (shift B left) will set C = old bit7 of B, put 0 in bit0, and set Z if the new B is 0, while always clearing N and H ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). `RR C` (rotate C right through carry) will set C = old bit0 of C, and bit7 of C becomes the old carry; Z set if result 0, N=0, H=0, etc. The half-carry flag plays no role in shifts/rotates and is always reset.

- **Bit Test (BIT b,r)**: The `BIT b,r` instruction tests a single bit of a register or memory, effectively by ANDing it with that bit mask. It sets **Z = 1 if the bit tested was 0 (i.e. the result of the test is zero) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A))**, otherwise Z=0 if the bit was 1. This provides a convenient way to use the Z flag as an indicator of that bit. The **H flag is set to 1** for every BIT operation ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,of%20register%20r%20is%200)). This is a somewhat odd rule – a quirk carried over from the Z80 design. Essentially, Z80’s BIT was documented to set H=1 and N=0 always. The Game Boy follows suit: **N is reset to 0** (since this is not a subtraction) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,of%20register%20r%20is%200)), and **H is set** (hardwired to 1) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,of%20register%20r%20is%200)). C is not affected by BIT (it remains whatever it was) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=H%20)). The rationale for H=1 on BIT is largely historical (on the Z80, the H flag was repurposed to signal something internal for BIT operations, but it has no practical effect in Game Boy programming). It is simply an invariant: after any BIT test, H will be 1, N will be 0. For example, `BIT 3,B`: if bit 3 of B was 0, Z=1 (zero result), if bit 3 of B was 1, Z=0; either way N=0, H=1, and C is unchanged.

- **Flag Toggle/Set (CCF, SCF)**: Two special ALU-related instructions explicitly adjust the carry flag. `SCF` (Set Carry Flag) will set **C = 1**, and it also defines **N = 0 and H = 0** ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)). Essentially SCF says “make carry true, and treat it as an addition operation” (so N=0, H=0). `CCF` (Complement Carry Flag) flips the carry – if C was 1 it becomes 0, and vice versa. On the Game Boy, CCF will **clear N = 0, clear H = 0**, and C is complemented ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). (On the original Z80, CCF also put the *old* carry into the H flag, but the Game Boy’s documentation and behavior indicate H is simply reset to 0 on CCF ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). This is a minor deviation from Z80, simplifying the flag logic.) Neither CCF nor SCF affects the Z flag at all ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)) (Z remains whatever it was). So after a CCF, the carry is inverted, N and H are 0; after SCF, carry is set to 1, N=0, H=0. These instructions don’t use the ALU’s adder per se, but they interact with the flag latch logic directly.

- **Decimal Adjust (DAA)**: The DAA instruction is perhaps the most *quirky* ALU-related operation on the SM83. DAA exists to support binary-coded decimal (BCD) arithmetic. After an addition or subtraction in BCD, the accumulator A might contain a non-BCD result, and DAA adjusts it to a correct BCD number using the H and C flags as guides. On the Game Boy, DAA looks at the *previous* operation’s N (add/subtract flag) to determine whether it should add or subtract the adjustment, and uses H and C to see if half or full carry occurred in nibbles. The logic internally is a bit complex (essentially adding 0x06 to the low nibble if H or low nibble >9, and adding 0x60 to the high nibble if C or high >9, with subtleties for subtraction). From a flags perspective, **DAA does not have an innate “formula” like other ops; it adjusts flags based on the corrected result**. Specifically, **Z is set if A becomes 0 after the adjustment** ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). **N is not affected** – it remains the same as it was before DAA (because DAA is defined to follow an addition or subtraction, and it doesn’t change the nature of that operation) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,A%20is%20zero)). **H is always cleared to 0 by DAA** ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). This makes sense because after adjusting, the half-carry nibble info is no longer relevant (the result is fixed to BCD digits). The **C flag is set or cleared to reflect any carry-out from the upper nibble after the BCD adjustment** ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). In practice, if there was an initial carry or the adjustment added 0x60, C will end up set. If not, C stays clear. The Half Carry flag being cleared is interesting – even if a half-carry was used in the adjustment, the final state of H is defined to 0 on Game Boy (the Z80 would sometimes leave it in a particular state after DAA, but on LR35902 it’s documented as reset). DAA is an outlier because it’s *data-dependent* on flags: e.g., after an addition where the low nibble went past 9 or H was set, DAA will add 6 to that nibble. Thus, DAA’s internal use of the ALU is conditional. Reverse-engineered analyses have shown that the CPU likely has a small lookup or logic block for DAA that effectively computes the necessary correction in one cycle. From the ISA perspective, one just needs to remember to properly set H and C before calling DAA (which the CPU does automatically on ADD/SUB) and then interpret the adjusted flags after. It’s worth noting that DAA is only defined for binary-coded decimal adjustments on an 8-bit value (the accumulator). It is *ineffective for 16-bit BCD* – as Pan Docs notes, since 16-bit values have 4 BCD digits and only one H and C flag, you can’t adjust a 16-bit BCD with a single DAA ([CPU Registers and Flags - Pan Docs](https://gbdev.io/pandocs/CPU_Registers_and_Flags.html#:~:text=previous%20instruction%20has%20been%20a,flag%29%20has%20limits)).

## Quirks, Constraints, and Anomalies  
With the above understanding of the ALU and flags, we can appreciate a few quirky behaviors in the Game Boy’s instruction set that stem from the hardware design:

- **4-Bit ALU Timing and “Half-Carry”**: The very concept of the Half Carry flag (H) is directly tied to the ALU’s nibble-by-nibble operation. In an 8-bit CPU with an 8-bit-wide ALU, one might have had to explicitly detect a nibble overflow for DAA. But in the SM83, the hardware *naturally* produces a half-carry signal as a byproduct of doing the low 4-bit addition first. This made implementing BCD support easier. However, it also introduces some programmer-visible quirks: for instance, in a 16-bit add, what constitutes a “half carry” is not immediately obvious until you realize it’s just the carry from bit 11 of the 16-bit operation (i.e., the carry from the lower 12 bits) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=N%20)). Similarly, instructions like `ADD SP, e` use only the low 8-bit half-carry – a design decision likely made to simplify the hardware (treat the low byte add as the flag-setting event). This is why, as confirmed by experiments, `ADD SP,e` sets H based on a carry from bit3 of the *low* byte and ignores a carry from bit 11 altogether ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=)) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=For%20,bit%2011%20to%20bit%2012)). To reason about such instructions, it helps to “think like the ALU”: break the operation into an 8-bit low part and an 8-bit high part, and know that H will reflect a carry between nibble 3→4 of whichever 8-bit chunk was designated for flag calculation ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=It%20depends%20on%20the%20instruction%2C,3%20of%20the%20high%20byte)).

- **Flag Consistency and Inconsistency**: Most arithmetic/logic instructions follow consistent rules for flags (as detailed above). But a few stand out: The BIT instruction always setting H=1 (and N=0) regardless of data can seem odd – it’s effectively an arbitrary constant. This originates from the Z80 using the H flag as a parity/overflow helper in BIT instructions (on Z80, undocumented use of flag bits), but on Game Boy parity/overflow flag isn’t present, so H=1 is just a vestige. Another is **CPL (complement accumulator)**: it sets N=1 and H=1 ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). Why would flipping bits in A be flagged as a “half-carry”? The answer is it isn’t – but Z80 decided to set those flags to indicate that “auxiliary flags” might need attention if the value in A was meant to be adjusted later. Essentially, CPL is defined to set N and H to 1 as a form of flag annotation (perhaps to signal that a two’s complement has been performed, which matters in BCD correction sequences). On the Game Boy, this has little practical use except that it’s part of the documented behavior: after `CPL`, the carry and zero remain unchanged, but N and H will both be 1 ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). This can matter if a DAA is done afterward: since N=1 tells DAA it was a subtraction, and H=1 would tell DAA there was a half-borrow (in context of subtraction) – though doing CPL followed by DAA is an unusual sequence. In any case, it’s a quirk born from compatibility with the Z80 design.

- **SCF/CCF Differences from Z80**: On a full Z80, the CCF instruction copies the previous C into H (and clears N). On the Game Boy’s SM83, CCF simply clears H (H=0) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)). This simplification likely reflects that the half-carry flag is only used for BCD, and complementing the carry doesn’t meaningfully relate to BCD correction. Clearing it avoids confusion. The SCF instruction on Game Boy matches Z80’s behavior (H and N cleared, C set) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)). The net result is that after either setting or complementing carry, the half-carry flag will always be 0 on this CPU. This is a subtle difference a veteran Z80 coder might notice, but it rarely affects real-world Game Boy code since H is only relevant immediately around DAA, and CCF/SCF aren’t used in those scenarios.

- **No Overflow/Parity Flag**: The original Intel 8080 had a parity flag, and the Z80 repurposed it as a parity/overflow (P/V) flag. The LR35902/SM83 omits this flag entirely – the bit is not present in F. This means there is no direct flag for two’s-complement overflow from arithmetic (which some CPUs provide). Any needed overflow detection (rare in typical GB code like games) would have to be done in software. By dropping this flag, the ALU hardware is simplified: the parity calculation circuitry (which in the Z80 ALU is an extra XOR network) is gone, and overflow isn’t computed. This is one constraint: certain complex numerical algorithms might be trickier, but in practice game logic seldom needs an overflow flag. The simplification likely saved a bit of die area and made flag logic more straightforward (just Z, N, H, C to update).

- **Instruction Timing Anomalies – The HALT bug**: Although not directly tied to the ALU, it’s worth mentioning the famous “HALT bug” as a curiosity of the SM83 control logic. If interrupts are disabled (IME=0) and a HALT instruction is executed, the CPU doesn’t advance the PC correctly and will essentially freeze on the next opcode fetch, reading the same byte twice. This is an unintended edge case in the control logic when the HALT is canceled by a pending but disabled interrupt. It shows that even in a simple CPU, corner-case behaviors exist beyond the documented ISA. Emulators must replicate this quirk for certain games that rely on it. While the ALU isn’t involved, it’s an example of how reverse-engineering the CPU revealed behavior that isn’t described in official manuals – a reminder that the Game Boy’s simplicity still hides a few surprises. (Another non-ALU quirk is how STOP is effectively a two-byte instruction `0x10 0x00`, a detail that appears in some docs but is a curiosity of the decoder.)

- **Undefined Behaviors and Useless Flags**: Some instructions update flags in ways that aren’t necessarily *useful*, but come “for free” from the ALU hardware. For example, when adding HL, the Z flag isn’t touched – which could be seen as an arbitrary decision, but it’s likely because the designers decided not to devote extra hardware to clearing Z for 16-bit ops (and on Z80 the Sign/Zero/Parity were left from the last 8-bit result). On Game Boy, the Z flag after an `ADD HL,DE` will still reflect whatever the last 8-bit operation was. This could be considered an anomaly (one might expect Z=0 always or something), but it’s consistent with Z80 behavior and has essentially no impact on typical code (since one wouldn’t check Z after an ADD HL). Another example: after a `BIT` test, the carry flag is left unchanged – you might think testing a bit shouldn’t have anything to do with carry, and indeed it doesn’t, which is why C is untouched. But if a programmer naively expected all bits of F to be fresh after a BIT, they’d be mistaken – C still holds whatever it did before. Fortunately, official documentation and Pan Docs make these effects clear, and most programmers/assemblers knew the Z80 heritage.

- **Emulator and Die-shot Confirmations**: Modern decap and tracing projects have given us visual confirmation of the ALU’s implementation. Die photos of the DMG-CPU (Game Boy CPU) show a repeating structure consistent with a 4-bit ALU: there are four identical slices in the arithmetic unit region ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20image%20above%20zooms%20in,individual%20gates%20in%20the%20ALU)), corresponding to the 4 bits. Ken Shirriff’s reverse-engineering of the Z80 ALU has illustrated how each slice contains transistors forming a full adder with logic gates to produce AND/OR/XOR and carry propagation ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20silicon%20that%20implements%20the,ALU)) ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20ALU%27s%20core%20computation%20circuit)). While a full-blown transistor-level analysis of the LR35902 isn’t publicly available to the same granularity, we can be confident it is extremely similar to the Z80’s, given the functional behavior matches so closely (aside from the removed flag). The presence of the separate incrementer for 16-bit operations is also something learned from reverse-engineering and emulator authors: it’s the only way to explain how 16-bit increments take only 8 clocks and don’t affect flags ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=The%20SM83%20CPU%20core%20used,Basically)). In designs like these, the Program Counter is often incremented by dedicated circuitry every cycle, and that same circuitry can be reused for instructions like INC HL. This parallel hardware is another form of optimization that doesn’t show up in the ISA except as a performance difference and the lack of flag updates. 

In conclusion, the Sharp SM83’s ALU is a study in efficient 1980s CPU design – by using a 4-bit slice ALU and clever flag logic, it achieves all the necessary 8-bit operations (and even multi-byte operations) while minimizing hardware. This implementation directly influences how instructions are defined and behave: everything from the meaning of the half-carry flag to the exact flags set by “odd” instructions like DAA or BIT is rooted in what the hardware is doing under the hood. Reverse-engineering efforts, through decapped die images and careful testing, have allowed us to connect the dots from transistor-level details (e.g. which gates output a carry) to the user-visible quirks of the Game Boy’s instruction set. The result is an intuitive understanding that *bridges the abstraction layers*: we see that the ALU’s design not only performs math, but in doing so it *creates* the conditions (like half-carries and carry-outs) that the architecture then exposes as flags. And those flags, in turn, are what the software relies on for branching logic (Zero/Carry conditions) and special corrections (DAA). Thus, the ALU design choices constrained and enabled the ISA features: for instance, BCD support was enabled by including the half-carry and a DAA instruction, while multi-bit rotates were simplified by dropping parity/overflow flags. The few anomalies and surprises (like specific flag behaviors) can all be traced to conscious decisions to simplify hardware or maintain Z80 compatibility. All told, the Game Boy’s ALU and flag system, once “deconstructed,” provide a beautiful example of **vertical engineering** – where decisions at the transistor level ripple up through microarchitecture and become visible (and sometimes puzzling) details of the programming model. By understanding it deeply, one gains appreciation for the engineers’ balancing act and can better anticipate the console’s behavior in both expected and unexpected scenarios.

## References and Sources  

- Pan Docs – *Game Boy Complete Technical Reference*, particularly the CPU and Instructions sections ([CPU Registers and Flags - Pan Docs](https://gbdev.io/pandocs/CPU_Registers_and_Flags.html#:~:text=These%20flags%20are%20used%20by,flag%29%20has%20limits)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,Set%20if%20A%20%3D%20n)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)).  
- GameBoy CPU Manual (GBdev/Devrs reference sheet) – Details of each opcode and flag effects ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Flags%20affected%3A)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20)) ([



		GameBoy CPU InstructionSet Sheet (GCISheet)



	](http://www.devrs.com/gb/files/GBCPU_Instr.html#:~:text=Z%20,is%20zero)).  
- Ken Shirriff’s Blog – *The Z-80 has a 4-bit ALU. Here’s how it works.* (September 2013), for transistor-level and block-diagram insight into the ALU slices ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20two%20operands%20go%20to,the%20second%20computation%20if%20needed)) ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20image%20above%20zooms%20in,individual%20gates%20in%20the%20ALU)) ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=,so%20the%20lower%20AND%20gate)).  
- Stack Exchange (Retrocomputing/Stack Overflow) – threads analyzing half-carry behavior and SM83 implementation details ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=For%20each%20case%20I%20store,those%20bytes%20on%20the%20screen)) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=The%20SM83%20CPU%20core%20used,Basically)) ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=We%20don%27t%20yet%20have%20a,then%20load%20SP%20from%20it)).  
- Game Boy “Boot ROM Reverse Engineering” and emulator developer notes – e.g. discussion on nesdev forums about the SM83 vs Z80, confirming 4-bit ALU and pipeline behavior ([
The Nintendo® Game Boy™, Part 1: The Intel 8080 and the Zilog Z80. | RealBoy	](https://realboyemulator.wordpress.com/2013/01/01/the-nintendo-game-boy-1/#:~:text=Thank%20you%20very%20much%20for,complicated%20than%20what%20I%20first)).  
- Craig Bishop’s FPGA Game Boy project logs – which include an architectural block diagram and notes derived from a Game Boy development book ([
    
    FPGA Game Boy Part 3: ALU and some microcode · Craig J. Bishop
    
  ](https://craigjb.com/2018/04/13/alu-microcode/#:~:text=operand%20to%20the%20ALU%20comes,operations%20update%20all%20flag%20bits)).  
- Real hardware experimentation (e.g. Michael’s answer on Stack Overflow testing `ADD SP,e` on real GBs) confirming flag results ([assembly - Game Boy: Half-carry flag and 16-bit instructions (especially opcode 0xE8) - Stack Overflow](https://stackoverflow.com/questions/57958631/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8#:~:text=For%20each%20case%20I%20store,those%20bytes%20on%20the%20screen)).  
- Decapped Game Boy CPU visual inspection and comparisons to decapped Z80 (various sources) ([The Z-80 has a 4-bit ALU. Here's how it works.](http://www.righto.com/2013/09/the-z-80-has-4-bit-alu-heres-how-it.html#:~:text=The%20silicon%20that%20implements%20the,ALU)).