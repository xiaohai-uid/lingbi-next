package main

import (
	"crypto/ed25519"
	"log"
	"net/http"

	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/auth"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/entitlement"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/server"
)

func main() {
	address := ":8080"
	log.Printf("LingBi Cloud listening on %s", address)
	_, privateKey, err := ed25519.GenerateKey(nil)
	if err != nil {
		log.Fatal(err)
	}
	handler := server.New(auth.NewService(), entitlement.New(privateKey))
	if err := http.ListenAndServe(address, handler); err != nil {
		log.Fatal(err)
	}
}
