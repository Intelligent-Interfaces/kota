package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSearchFile(t *testing.T) {
	// Create a temporary directory
	tempDir, err := os.MkdirTemp("", "wiki_test")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tempDir)

	// Create a temporary markdown file with query occurrences
	tempFile := filepath.Join(tempDir, "test_article.md")
	content := "This is a test wiki page. It contains the word apple three times. apple, apple."
	if err := os.WriteFile(tempFile, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	resultsChan := make(chan SearchResult, 10)

	// Search for "apple" (lowercase)
	searchFile(tempFile, "apple", resultsChan)
	close(resultsChan)

	var results []SearchResult
	for res := range resultsChan {
		results = append(results, res)
	}

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %d", len(results))
	}

	res := results[0]
	if res.Path != tempFile {
		t.Errorf("expected path %v, got %v", tempFile, res.Path)
	}
	if res.Score != 3 {
		t.Errorf("expected score 3, got %v", res.Score)
	}
	if res.Snippet == "" {
		t.Error("expected non-empty snippet")
	}
}

func TestSearchFileNoMatch(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "wiki_test_empty")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tempDir)

	tempFile := filepath.Join(tempDir, "no_match.md")
	content := "This page has banana but no other fruits."
	if err := os.WriteFile(tempFile, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	resultsChan := make(chan SearchResult, 10)
	searchFile(tempFile, "orange", resultsChan)
	close(resultsChan)

	var results []SearchResult
	for res := range resultsChan {
		results = append(results, res)
	}

	if len(results) != 0 {
		t.Errorf("expected 0 results, got %d", len(results))
	}
}
