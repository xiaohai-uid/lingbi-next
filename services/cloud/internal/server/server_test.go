package server

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestHealthz(t *testing.T) {
	request := httptest.NewRequest(http.MethodGet, "/healthz", nil)
	recorder := httptest.NewRecorder()

	New().ServeHTTP(recorder, request)

	if recorder.Code != http.StatusOK {
		t.Fatalf("healthz status = %d, want %d", recorder.Code, http.StatusOK)
	}
	if recorder.Body.String() != "ok" {
		t.Fatalf("healthz body = %q, want ok", recorder.Body.String())
	}
}

func TestReadyz(t *testing.T) {
	request := httptest.NewRequest(http.MethodGet, "/readyz", nil)
	recorder := httptest.NewRecorder()

	New().ServeHTTP(recorder, request)

	if recorder.Code != http.StatusOK {
		t.Fatalf("readyz status = %d, want %d", recorder.Code, http.StatusOK)
	}
	if recorder.Body.String() != "ready" {
		t.Fatalf("readyz body = %q, want ready", recorder.Body.String())
	}
}
