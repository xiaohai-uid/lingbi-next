package main

import (
	"crypto/ed25519"
	"log"
	"net/http"

	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/auth"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/billing"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/entitlement"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/releases"
	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/server"
)

func main() {
	address := ":8080"
	log.Printf("LingBi Cloud listening on %s", address)
	_, privateKey, err := ed25519.GenerateKey(nil)
	if err != nil {
		log.Fatal(err)
	}
	releaseService := releases.NewService(releases.NewMemoryStorage(
		releases.Release{
			Version:     "0.1.0",
			DownloadURL: "https://download.example/lingbi.exe",
			SHA256:      "placeholder",
		},
	))
	checkoutService := billing.NewCheckoutService(
		billing.SandboxProvider{},
		billing.NewWebhookService(billing.NewMemoryEntitlementMutator()),
	)
	handler := server.New(
		auth.NewService(),
		entitlement.New(privateKey),
		releaseService,
		checkoutService,
	)
	if err := http.ListenAndServe(address, handler); err != nil {
		log.Fatal(err)
	}
}
