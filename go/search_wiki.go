package main

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

type SearchResult struct {
	Path    string `json:"path"`
	Score   int    `json:"score"`
	Snippet string `json:"snippet"`
}

func main() {
	if len(os.Args) < 3 {
		fmt.Println("Usage: go run search_wiki.go <dir> <query>")
		os.Exit(1)
	}

	searchDir := os.Args[1]
	query := strings.ToLower(os.Args[2])

	var wg sync.WaitGroup
	resultsChan := make(chan SearchResult, 100)

	err := filepath.WalkDir(searchDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.IsDir() && strings.HasSuffix(d.Name(), ".md") {
			wg.Add(1)
			go func(filePath string) {
				defer wg.Done()
				searchFile(filePath, query, resultsChan)
			}(path)
		}
		return nil
	})

	if err != nil {
		fmt.Fprintf(os.Stderr, "Error walking path: %v\n", err)
		os.Exit(1)
	}

	go func() {
		wg.Wait()
		close(resultsChan)
	}()

	var results []SearchResult
	for res := range resultsChan {
		if res.Score > 0 {
			results = append(results, res)
		}
	}

	// Sort results by score descending
	for i := 0; i < len(results); i++ {
		for j := i + 1; j < len(results); j++ {
			if results[j].Score > results[i].Score {
				results[i], results[j] = results[j], results[i]
			}
		}
	}

	output, _ := json.MarshalIndent(results, "", "  ")
	fmt.Println(string(output))
}

func searchFile(path string, query string, resultsChan chan<- SearchResult) {
	contentBytes, err := os.ReadFile(path)
	if err != nil {
		return
	}

	content := strings.ToLower(string(contentBytes))
	count := strings.Count(content, query)

	if count > 0 {
		// Find first snippet
		idx := strings.Index(content, query)
		start := idx - 30
		if start < 0 {
			start = 0
		}
		end := idx + len(query) + 50
		if end > len(content) {
			end = len(content)
		}
		snippet := string(contentBytes[start:end])
		snippet = strings.ReplaceAll(snippet, "\n", " ")

		resultsChan <- SearchResult{
			Path:    path,
			Score:   count,
			Snippet: "... " + strings.TrimSpace(snippet) + " ...",
		}
	}
}
