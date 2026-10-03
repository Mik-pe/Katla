package main

import "core:fmt"
import icons "../icons"

main :: proc() {
    for icon in icons.ALL_ICONS { fmt.printf("%s %x\n",icon.name,u32(icon.codepoint)) }
    fmt.println("COMMON")
    for icon in icons.COMMON_ICONS { fmt.printf("%x\n",u32(icon)) }
}
