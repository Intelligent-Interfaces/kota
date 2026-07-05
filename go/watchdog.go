package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"runtime"
	"time"
)

type ResourceStats struct {
	Timestamp    string `json:"timestamp"`
	OS           string `json:"os"`
	Architecture string `json:"architecture"`
	CPUs         int    `json:"cpus"`
	GoRoutines   int    `json:"goroutines"`
	AllocMB      uint64 `json:"alloc_mb"`
	SysMB        uint64 `json:"sys_mb"`
	NumGC        uint32 `json:"num_gc"`
	VRAMStatus   string `json:"vram_status"`
	Status       string `json:"status"`
}

func main() {
	http.HandleFunc("/status", statusHandler)
	fmt.Println("Watchdog resource daemon listening on :8766...")
	if err := http.ListenAndServe(":8766", nil); err != nil {
		fmt.Printf("Error starting watchdog: %v\n", err)
	}
}

func statusHandler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Access-Control-Allow-Origin", "*")

	var m runtime.MemStats
	runtime.ReadMemStats(&m)

	stats := ResourceStats{
		Timestamp:    time.Now().Format(time.RFC3339),
		OS:           runtime.GOOS,
		Architecture: runtime.GOARCH,
		CPUs:         runtime.NumCPU(),
		GoRoutines:   runtime.NumGoroutine(),
		AllocMB:      m.Alloc / 1024 / 1024,
		SysMB:        m.Sys / 1024 / 1024,
		NumGC:        m.NumGC,
		VRAMStatus:   "Optimized - VRAM buffer stable", // Mock proxy status
		Status:       "OK",
	}

	json.NewEncoder(w).Encode(stats)
}
