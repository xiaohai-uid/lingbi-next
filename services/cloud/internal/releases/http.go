package releases

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
)

type Service struct {
	storage Storage
}

func NewService(storage Storage) *Service {
	return &Service{storage: storage}
}

func (s *Service) LatestHandler(writer http.ResponseWriter, request *http.Request) {
	release, err := s.storage.Latest(request.Context())
	if err != nil {
		writeError(writer, http.StatusNotFound, "no releases")
		return
	}
	writeJSON(writer, http.StatusOK, release)
}

func (s *Service) ByVersionHandler(writer http.ResponseWriter, request *http.Request) {
	version := chi.URLParam(request, "version")
	release, err := s.storage.ByVersion(request.Context(), version)
	if err != nil {
		writeError(writer, http.StatusNotFound, "release not found")
		return
	}
	writeJSON(writer, http.StatusOK, release)
}

func (s *Service) WindowsDownloadHandler(writer http.ResponseWriter, request *http.Request) {
	release, err := s.storage.WindowsX86_64(request.Context())
	if err != nil {
		writeError(writer, http.StatusNotFound, "windows release not found")
		return
	}
	writeJSON(writer, http.StatusOK, release)
}

func writeJSON(writer http.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func writeError(writer http.ResponseWriter, status int, message string) {
	writeJSON(writer, status, map[string]string{"error": message})
}
