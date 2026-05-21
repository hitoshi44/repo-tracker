// repo-tracker (Go版): GitLab API から repo メタ + 追跡ファイルを集めて site/data/ に書き出す。
// 使い方: repo-tracker [path/to/config.yml]   (省略時 config.yml)
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"repo-tracker/internal/config"
	"repo-tracker/internal/gitlab"
	"repo-tracker/internal/model"
	"repo-tracker/internal/parser"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

type reposJson struct {
	FetchedAt string             `json:"fetched_at"`
	Repos     []model.Repository `json:"repos"`
}

func run() error {
	configPath := "config.yml"
	if len(os.Args) > 1 {
		configPath = os.Args[1]
	}
	cfg, err := config.LoadConfig(configPath)
	if err != nil {
		return err
	}
	reposPath := config.ResolveRelative(configPath, cfg.ReposFile)
	entries, err := config.LoadRepos(reposPath, cfg.Defaults.Nest, cfg.GitLab.URL)
	if err != nil {
		return err
	}
	if len(entries) == 0 {
		return fmt.Errorf("no repos in %s", reposPath)
	}

	outDir := config.ResolveRelative(configPath, cfg.OutputDir)
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}

	baseURL := strings.TrimRight(cfg.GitLab.URL, "/")
	token := os.Getenv(cfg.GitLab.TokenEnv)
	targets := []string{".gitlab-ci.yml", "package.json", "pom.xml"}
	client := gitlab.New(token)

	fetchedAt := time.Now().UTC().Format(time.RFC3339)

	fmt.Printf("config: %s\n", configPath)
	fmt.Printf("repos:  %s (%d entries)\n", reposPath, len(entries))
	fmt.Printf("output: %s\n", outDir)

	var (
		repositories []model.Repository
		ciRaws       []model.RawEntry
		pkgRaws      []model.RawEntry
		pomRaws      []model.RawEntry
		fileCount    int
	)

	for _, entry := range entries {
		meta, err := client.FetchProjectMeta(baseURL, entry.Path)
		if err != nil {
			return fmt.Errorf("fetch meta %s: %w", entry.Path, err)
		}
		files, err := client.ListTrackedFiles(baseURL, entry.Path, entry.Nest, targets)
		if err != nil {
			return fmt.Errorf("list tracked %s: %w", entry.Path, err)
		}

		for _, f := range files {
			raw, err := client.FetchFile(baseURL, meta.PathWithNamespace, meta.DefaultBranch, f.Path)
			if err != nil {
				return fmt.Errorf("fetch file %s/%s: %w", entry.Path, f.Path, err)
			}
			rawEntry := model.RawEntry{RepoID: meta.ID, Path: f.Path, Raw: raw.Raw}
			switch f.Type {
			case model.KindGitlabCi:
				ciRaws = append(ciRaws, rawEntry)
			case model.KindPackageJson:
				pkgRaws = append(pkgRaws, rawEntry)
			case model.KindPomXml:
				pomRaws = append(pomRaws, rawEntry)
			}

			parsed, err := parser.Parse(f.Type, raw.Raw)
			if err != nil {
				return fmt.Errorf("parse %s (%s): %w", f.Path, f.Type, err)
			}
			tracked := model.TrackedFile{
				RepoID:  meta.ID,
				Path:    f.Path,
				Type:    f.Type,
				BlobSHA: raw.BlobSHA,
				Size:    raw.Size,
				Raw:     raw.Raw,
				Parsed:  parsed,
			}
			outPath := filepath.Join(outDir, "files", fmt.Sprintf("%d", meta.ID), f.Path+".json")
			if err := writeJSON(outPath, tracked); err != nil {
				return err
			}
			fileCount++
		}

		repositories = append(repositories, model.Repository{
			ID:        meta.ID,
			Path:      meta.PathWithNamespace,
			Name:      meta.Name,
			URL:       meta.WebURL,
			FetchedAt: fetchedAt,
			Files:     files,
		})
	}

	rj := reposJson{FetchedAt: fetchedAt, Repos: repositories}
	if err := writeJSON(filepath.Join(outDir, "repos.json"), rj); err != nil {
		return err
	}
	if err := writeJSON(filepath.Join(outDir, "ci-raws.json"), nonNilSlice(ciRaws)); err != nil {
		return err
	}
	if err := writeJSON(filepath.Join(outDir, "pkg-raws.json"), nonNilSlice(pkgRaws)); err != nil {
		return err
	}
	if err := writeJSON(filepath.Join(outDir, "pom-raws.json"), nonNilSlice(pomRaws)); err != nil {
		return err
	}

	fmt.Printf("wrote %s: repos.json, ci-raws.json (%d), pkg-raws.json (%d), pom-raws.json (%d), and %d file(s) under files/\n",
		outDir, len(ciRaws), len(pkgRaws), len(pomRaws), fileCount)
	for _, r := range repositories {
		fmt.Printf("  %s (%d file(s))\n", r.Path, len(r.Files))
	}
	return nil
}

func writeJSON(path string, v any) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	buf, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, buf, 0o644)
}

// JSON で [] を出すために、nil slice を空 slice に置き換える。
func nonNilSlice(xs []model.RawEntry) []model.RawEntry {
	if xs == nil {
		return []model.RawEntry{}
	}
	return xs
}
