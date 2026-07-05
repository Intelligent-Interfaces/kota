package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"strings"
)

type CoordinatorEvent struct {
	AgentID string `json:"agent_id"`
	Action  string `json:"action"`
	Payload string `json:"payload"`
}

func main() {
	if len(os.Args) < 2 {
		fmt.Println("Usage: go run coordinator.go <command> [args]")
		fmt.Println("Commands:")
		fmt.Println("  spawn <model>   Spawns a new Kota instance in background")
		fmt.Println("  ping            Pings the local resource watchdog")
		os.Exit(1)
	}

	command := os.Args[1]
	switch command {
	case "spawn":
		if len(os.Args) < 3 {
			fmt.Println("Usage: go run coordinator.go spawn <model>")
			os.Exit(1)
		}
		spawnAgent(os.Args[2])
	case "ping":
		pingWatchdog()
	default:
		fmt.Printf("Unknown command: %s\n", command)
	}
}

func spawnAgent(model string) {
	cmd := exec.Command("cargo", "run", "--", "--model", model)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	fmt.Printf("Coordinator spawning Kota agent with model: %s...\n", model)
	err := cmd.Start()
	if err != nil {
		fmt.Printf("Error spawning agent: %v\n", err)
		return
	}
	fmt.Printf("Agent successfully spawned in background. PID: %d\n", cmd.Process.Pid)
}

func pingWatchdog() {
	resp, err := http.Get("http://localhost:8766/status")
	if err != nil {
		fmt.Printf("Error contacting watchdog: %v. Is go run watchdog.go running?\n", err)
		return
	}
	defer resp.Body.Close()

	var buf bytes.Buffer
	buf.ReadFrom(resp.Body)

	var pretty bytes.Buffer
	json.Indent(&pretty, buf.Bytes(), "", "  ")

	fmt.Println("Watchdog Status Response:")
	fmt.Println(strings.TrimSpace(pretty.String()))
}

// ReadInput reads line from stdin
func ReadInput() string {
	reader := bufio.NewReader(os.Stdin)
	text, _ := reader.ReadString('\n')
	return strings.TrimSpace(text)
}
