package main

import (
	"log"
	"net/http"

	"github.com/xiaohai-uid/lingbi-next/services/cloud/internal/server"
)

func main() {
	address := ":8080"
	log.Printf("LingBi Cloud listening on %s", address)
	if err := http.ListenAndServe(address, server.New()); err != nil {
		log.Fatal(err)
	}
}
