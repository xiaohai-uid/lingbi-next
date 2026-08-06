package entitlement

import (
	"crypto/ed25519"
	"testing"
)

func TestIssueAndVerify(t *testing.T) {
	_, privateKey, err := ed25519.GenerateKey(nil)
	if err != nil {
		t.Fatal(err)
	}
	service := New(privateKey)

	token, err := service.Issue("user-1", "pro", []string{"offline"})
	if err != nil {
		t.Fatal(err)
	}
	claims, err := service.Verify(token)
	if err != nil {
		t.Fatal(err)
	}
	if claims.Sub != "user-1" || claims.Plan != "pro" {
		t.Fatalf("claims = %+v", claims)
	}
}

func TestTamperedTokenRejected(t *testing.T) {
	_, privateKey, _ := ed25519.GenerateKey(nil)
	service := New(privateKey)
	token, _ := service.Issue("user-1", "free", nil)

	if token[:2] == "AA" {
		token = "BB" + token[2:]
	} else {
		token = "AA" + token[2:]
	}

	if _, err := service.Verify(token); err == nil {
		t.Fatal("tampered entitlement must be rejected")
	}
}
