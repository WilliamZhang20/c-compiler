# A Custom Malloc

Some [criticize](https://www.youtube.com/watch?v=jgiMagdjA1s) the usage of malloc/free.

So this directory contains a Windows-Specific custom malloc. Most of it is quite simple since it abstracts most gritty details to the Windows Memory API.

Key Features
1. Buffer Overflow Detection
- Places "canaries" (magic values 0xABCD1234) before and after allocated blocks
- Checks on free() if canaries are corrupted (indicating buffer overflow)

2. Memory Corruption Detection
- Uses magic number 0x534146454D414C43 ("SAFEMALC") to identify valid blocks
- Computes checksums of block headers to detect tampering
- Validates both magic and checksum on every free()

3. Use-After-Free Detection
- Poisons freed memory with 0xDE bytes
- Makes it more likely to crash if freed memory is accessed
- Uses VirtualFree() to release entire pages (making future access fault)

4. Metadata Protection
- Stores allocation size, magic number, and checksum in header
- Verifies integrity before each free operation