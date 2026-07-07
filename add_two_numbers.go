package main

import (
	"fmt"
	"strconv"
)

func main() {
	var a, b float64
	fmt.Print("Enter first number: ")
	fmt.Scanln(&a)
	fmt.Print("Enter second number: ")
	fmt.Scanln(&b)
	sum := a + b
	fmt.Printf("Sum: %f\n", sum)
}