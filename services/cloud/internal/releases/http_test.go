package releases

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/go-chi/chi/v5"
)

func TestReleaseHandlers(t *testing.T) {
	service := NewService(NewMemoryStorage(
		Release{
			Version:     "0.1.0",
			DownloadURL: "https://download.example/lingbi.exe",
			SHA256:      "abc",
		},
	))
	router := chi.NewRouter()
	router.Get("/v1/releases/latest", service.LatestHandler)
	router.Get("/v1/releases/{version}", service.ByVersionHandler)
	router.Get("/v1/download/windows/x86_64", service.WindowsDownloadHandler)

	tests := []struct {
		path string
		code int
	}{
		{"/v1/releases/latest", http.StatusOK},
		{"/v1/releases/0.1.0", http.StatusOK},
		{"/v1/releases/9.9.9", http.StatusNotFound},
		{"/v1/download/windows/x86_64", http.StatusOK},
	}
	for _, test := range tests {
		request := httptest.NewRequest(http.MethodGet, test.path, nil)
		recorder := httptest.NewRecorder()
		router.ServeHTTP(recorder, request)
		if recorder.Code != test.code {
			t.Fatalf("%s status = %d, want %d", test.path, recorder.Code, test.code)
		}
	}
}
