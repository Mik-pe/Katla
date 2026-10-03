package main
import "core:fmt"
import workload "../workload"
main :: proc() { fmt.printf("%.0f\n",workload.run()) }
