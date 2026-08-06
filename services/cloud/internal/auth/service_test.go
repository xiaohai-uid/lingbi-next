package auth

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestLoginRefreshLogoutMe(t *testing.T) {
	service := NewService()
	if _, err := service.Register("user@example.com", "password123"); err != nil {
		t.Fatal(err)
	}
	access, refresh, err := service.Login("user@example.com", "password123")
	if err != nil {
		t.Fatal(err)
	}
	user, err := service.Me(access)
	if err != nil || user.Email != "user@example.com" {
		t.Fatalf("Me = %v, %v", user, err)
	}
	nextAccess, nextRefresh, err := service.Refresh(refresh)
	if err != nil {
		t.Fatal(err)
	}
	if nextAccess == access || nextRefresh == refresh {
		t.Fatal("refresh token must rotate")
	}
	if err := service.Logout(nextRefresh); err != nil {
		t.Fatal(err)
	}
	if _, _, err := service.Refresh(nextRefresh); err == nil {
		t.Fatal("revoked refresh token must be rejected")
	}
}

func TestRefreshTokenIsStoredHashed(t *testing.T) {
	service := NewService()
	_, _ = service.Register("user@example.com", "password123")
	_, refresh, err := service.Login("user@example.com", "password123")
	if err != nil {
		t.Fatal(err)
	}
	service.mu.RLock()
	defer service.mu.RUnlock()
	for _, record := range service.refreshTokens {
		if record.TokenHash == refresh {
			t.Fatal("plaintext refresh token must not be stored")
		}
	}
}

func TestAuthHandlers(t *testing.T) {
	service := NewService()
	_, _ = service.Register("user@example.com", "password123")

	loginBody, _ := json.Marshal(loginRequest{
		Email:    "user@example.com",
		Password: "password123",
	})
	loginRequestHTTP := httptest.NewRequest(http.MethodPost, "/v1/auth/login", bytes.NewReader(loginBody))
	loginRecorder := httptest.NewRecorder()
	service.LoginHandler(loginRecorder, loginRequestHTTP)
	if loginRecorder.Code != http.StatusOK {
		t.Fatalf("login status = %d", loginRecorder.Code)
	}

	var tokens tokenResponse
	if err := json.Unmarshal(loginRecorder.Body.Bytes(), &tokens); err != nil {
		t.Fatal(err)
	}
	meRequest := httptest.NewRequest(http.MethodGet, "/v1/me", nil)
	meRequest.Header.Set("Authorization", "Bearer "+tokens.AccessToken)
	meRecorder := httptest.NewRecorder()
	service.MeHandler(meRecorder, meRequest)
	if meRecorder.Code != http.StatusOK {
		t.Fatalf("me status = %d", meRecorder.Code)
	}
}
