package server

import (
	"crypto/ed25519"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/auth"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/entitlement"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/releases"
)

func TestHealthz(t *testing.T) {
	request := httptest.NewRequest(http.MethodGet, "/healthz", nil)
	recorder := httptest.NewRecorder()

	New(auth.NewService(), newEntitlement(), newReleases()).ServeHTTP(recorder, request)

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

	New(auth.NewService(), newEntitlement(), newReleases()).ServeHTTP(recorder, request)

	if recorder.Code != http.StatusOK {
		t.Fatalf("readyz status = %d, want %d", recorder.Code, http.StatusOK)
	}
	if recorder.Body.String() != "ready" {
		t.Fatalf("readyz body = %q, want ready", recorder.Body.String())
	}
}

func TestEntitlementEndpoint(t *testing.T) {
	authService := auth.NewService()
	_, _ = authService.Register("user@example.com", "password123")
	accessToken, _, _ := authService.Login("user@example.com", "password123")
	request := httptest.NewRequest(http.MethodGet, "/v1/me/entitlement", nil)
	request.Header.Set("Authorization", "Bearer "+accessToken)
	recorder := httptest.NewRecorder()

	New(authService, newEntitlement(), newReleases()).ServeHTTP(recorder, request)

	if recorder.Code != http.StatusOK {
		t.Fatalf("entitlement status = %d, body = %s", recorder.Code, recorder.Body.String())
	}
}

func newEntitlement() *entitlement.Service {
	_, privateKey, _ := ed25519.GenerateKey(nil)
	return entitlement.New(privateKey)
}

func newReleases() *releases.Service {
	return releases.NewService(releases.NewMemoryStorage(
		releases.Release{Version: "0.1.0"},
	))
}
