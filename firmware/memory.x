MEMORY
{
  FLASH    : ORIGIN = 0x08000000,   LENGTH = 1024K
  RAM      : ORIGIN = 0x24000000,   LENGTH = 320K
  RAM_D3   : ORIGIN = 0x38000000,   LENGTH = 16K
}

SECTIONS
{
    .ram_d3 :
    {
        *(.ram_d3)
    } > RAM_D3
}