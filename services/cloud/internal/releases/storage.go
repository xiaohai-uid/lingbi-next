package releases

import (
	"context"
	"errors"
	"sort"
)

type Release struct {
	Version     string `json:"version"`
	DownloadURL string `json:"download_url"`
	SHA256      string `json:"sha256"`
}

type Storage interface {
	Latest(ctx context.Context) (Release, error)
	ByVersion(ctx context.Context, version string) (Release, error)
	WindowsX86_64(ctx context.Context) (Release, error)
}

type MemoryStorage struct {
	releases []Release
}

func NewMemoryStorage(releases ...Release) *MemoryStorage {
	return &MemoryStorage{releases: releases}
}

func (s *MemoryStorage) Latest(_ context.Context) (Release, error) {
	if len(s.releases) == 0 {
		return Release{}, errors.New("no releases")
	}
	sorted := append([]Release(nil), s.releases...)
	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].Version > sorted[j].Version
	})
	return sorted[0], nil
}

func (s *MemoryStorage) ByVersion(_ context.Context, version string) (Release, error) {
	for _, release := range s.releases {
		if release.Version == version {
			return release, nil
		}
	}
	return Release{}, errors.New("release not found")
}

func (s *MemoryStorage) WindowsX86_64(ctx context.Context) (Release, error) {
	return s.Latest(ctx)
}
