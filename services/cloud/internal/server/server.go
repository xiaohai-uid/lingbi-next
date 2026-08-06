package server

import (
	"encoding/json"
	"net/http"
	"strings"

	"github.com/go-chi/chi/v5"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/auth"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/entitlement"
)

func New(authService *auth.Service, entitlementService *entitlement.Service) http.Handler {
	router := chi.NewRouter()
	router.Get("/healthz", func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
		_, _ = writer.Write([]byte("ok"))
	})
	router.Get("/readyz", func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
		_, _ = writer.Write([]byte("ready"))
	})
	router.Post("/v1/auth/login", authService.LoginHandler)
	router.Post("/v1/auth/refresh", authService.RefreshHandler)
	router.Post("/v1/auth/logout", authService.LogoutHandler)
	router.Get("/v1/me", authService.MeHandler)
	router.Get("/v1/me/entitlement", func(writer http.ResponseWriter, request *http.Request) {
		accessToken := strings.TrimPrefix(request.Header.Get("Authorization"), "Bearer ")
		user, err := authService.Me(accessToken)
		if err != nil {
			writeError(writer, http.StatusUnauthorized, "invalid access token")
			return
		}
		token, err := entitlementService.Issue(user.ID, "free", []string{"local_manuscript"})
		if err != nil {
			writeError(writer, http.StatusInternalServerError, "entitlement issue failed")
			return
		}
		writeJSON(writer, http.StatusOK, map[string]string{"entitlement": token})
	})
	return router
}

func writeJSON(writer http.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func writeError(writer http.ResponseWriter, status int, message string) {
	writeJSON(writer, status, map[string]string{"error": message})
}
