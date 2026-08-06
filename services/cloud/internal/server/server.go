package server

import (
	"net/http"

	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/auth"
	"github.com/go-chi/chi/v5"
)

func New(authService *auth.Service) http.Handler {
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
	return router
}
