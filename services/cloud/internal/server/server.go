package server

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

func New() http.Handler {
	router := chi.NewRouter()
	router.Get("/healthz", func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
		_, _ = writer.Write([]byte("ok"))
	})
	router.Get("/readyz", func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
		_, _ = writer.Write([]byte("ready"))
	})
	return router
}
