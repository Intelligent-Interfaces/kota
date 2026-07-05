package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestStatusHandler(t *testing.T) {
	req, err := http.NewRequest("GET", "/status", nil)
	if err != nil {
		t.Fatal(err)
	}

	rr := httptest.NewRecorder()
	handler := http.HandlerFunc(statusHandler)

	handler.ServeHTTP(rr, req)

	// Check status code
	if status := rr.Code; status != http.StatusOK {
		t.Errorf("handler returned wrong status code: got %v want %v", status, http.StatusOK)
	}

	// Check content type
	contentType := rr.Header().Get("Content-Type")
	if contentType != "application/json" {
		t.Errorf("handler returned wrong content type: got %v want %v", contentType, "application/json")
	}

	// Parse JSON
	var stats ResourceStats
	if err := json.Unmarshal(rr.Body.Bytes(), &stats); err != nil {
		t.Fatalf("failed to parse JSON response: %v", err)
	}

	// Verify stats fields
	if stats.Status != "OK" {
		t.Errorf("expected Status 'OK', got %v", stats.Status)
	}
	if stats.CPUs <= 0 {
		t.Errorf("expected positive CPU count, got %v", stats.CPUs)
	}
	if stats.VRAMStatus == "" {
		t.Error("expected non-empty VRAM status")
	}
}
