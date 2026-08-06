package billing

import (
	"context"
	"net/http"
	"strings"
	"testing"
)

func TestSandboxCheckout(t *testing.T) {
	provider := SandboxProvider{}
	session, err := provider.CreateCheckout(context.Background(), CheckoutRequest{
		UserID: "user-1",
		Plan:   "pro",
	})
	if err != nil {
		t.Fatal(err)
	}
	if session.URL == "" {
		t.Fatal("checkout URL must not be empty")
	}
}

func TestWebhookIsIdempotent(t *testing.T) {
	mutator := NewMemoryEntitlementMutator()
	service := NewWebhookService(mutator)
	provider := SandboxProvider{}
	body := `{"id":"evt-1","user_id":"user-1","plan":"pro","kind":"checkout_completed"}`
	event, err := provider.VerifyWebhook(context.Background(), http.Header{}, []byte(body))
	if err != nil {
		t.Fatal(err)
	}

	for i := 0; i < 10; i++ {
		if err := service.Handle(event); err != nil {
			t.Fatal(err)
		}
	}

	if got := mutator.Count("evt-1"); got != 1 {
		t.Fatalf("entitlement applied %d times, want 1", got)
	}
}

func TestInvalidWebhookRejected(t *testing.T) {
	provider := SandboxProvider{}
	_, err := provider.VerifyWebhook(
		context.Background(),
		http.Header{},
		[]byte(strings.Repeat("x", 1)),
	)
	if err == nil {
		t.Fatal("invalid webhook must be rejected")
	}
}
