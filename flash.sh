esptool --chip esp32c6 -p /dev/cu.usbmodem101 -b 460800 \
  --before default-reset --after hard-reset write-flash \
  --flash-mode dio --flash-freq 80m --flash-size 4MB \
  0x0 build/bootloader/bootloader.bin \
  0x20000 build/intercom-matter.bin \
  0xc000 build/partition_table/partition-table.bin
