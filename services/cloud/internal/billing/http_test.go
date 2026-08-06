package billing

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestCheckoutAndWebhookHandlers(t *testing.T) {
	provider := SandboxProvider{}
	mutator := NewMemoryEntitlementMutator()
	service := NewCheckoutService(provider, NewWebhookService(mutator))

	checkoutBody := []byte(`{"user_id":"user-1","plan":"pro"}`)
	checkoutRequest := httptest.NewRequest(http.MethodPost, "/v1/checkout", bytes.NewReader(checkoutBody))
	checkoutRecorder := httptest.NewRecorder()
	service.CheckoutHandler(checkoutRecorder, checkoutRequest)
	if checkoutRecorder.Code != http.StatusOK {
		t.Fatalf("checkout status = %d", checkoutRecorder.Code)
	}

	webhookBody := []byte(`{"id":"evt-1","user_id":"user-1","plan":"pro","kind":"checkout_completed"}`)
	for i := 0; i < 10; i++ {
		webhookRequest := httptest.NewRequest(http.MethodPost, "/v1/billing/webhook", bytes.NewReader(webhookBody))
		webhookRecorder := httptest.NewRecorder()
		service.WebhookHandler(webhookRecorder, webhookRequest)
		if webhookRecorder.Code != http.StatusNoContent {
			t.Fatalf("webhook status = %d", webhookRecorder.Code)
		}
	}
	if got := mutator.Count("evt-1"); got != 1 {
		t.Fatalf("entitlement applied %d times, want 1", got)
	}
}

func TestInvalidCheckoutRequestRejected(t *testing.T) {
	service := NewCheckoutService(SandboxProvider{}, NewWebhookService(NewMemoryEntitlementMutator()))
	request := httptest.NewRequest(http.MethodPost, "/v1/checkout", bytes.NewReader([]byte("{")))
	recorder := httptest.NewRecorder()
	service.CheckoutHandler(recorder, request)
	if recorder.Code != http.StatusBadRequest {
		t.Fatalf("status = %d", recorder.Code)
	}
}

func TestInvalidWebhookBodyRejected(t *testing.T) {
	service := NewCheckoutService(SandboxProvider{}, NewWebhookService(NewMemoryEntitlementMutator()))
	request := httptest.NewRequest(http.MethodPost, "/v1/billing/webhook", bytes.NewReader([]byte("not-json")))
	recorder := httptest.NewRecorder()
	service.WebhookHandler(recorder, request)
	if recorder.Code != http.StatusBadRequest {
		t.Fatalf("status = %d", recorder.Code)
	}
}
