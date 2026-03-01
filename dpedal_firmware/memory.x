MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;

ASSERT(
    (_stack_start - _stack_end) >= 0x10000,
    "ERROR: stack less than 64 KiB. 64 KiB is plenty of stack, feel to decrease this if needed, but note that the dpedal firmware currently allocates large values on the stack."
);