package billing

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
)

type CheckoutRequest struct {
	UserID string
	Plan   string
}

type CheckoutSession struct {
	URL string
	ID  string
}

type BillingEventKind string

const (
	CheckoutCompleted     BillingEventKind = "checkout_completed"
	SubscriptionActivated BillingEventKind = "subscription_activated"
	SubscriptionRenewed   BillingEventKind = "subscription_renewed"
	SubscriptionCancelled BillingEventKind = "subscription_cancelled"
	PaymentFailed         BillingEventKind = "payment_failed"
	Refunded              BillingEventKind = "refunded"
)

type BillingEvent struct {
	ID     string           `json:"id"`
	UserID string           `json:"user_id"`
	Plan   string           `json:"plan"`
	Kind   BillingEventKind `json:"kind"`
}

type Provider interface {
	CreateCheckout(ctx context.Context, request CheckoutRequest) (CheckoutSession, error)
	VerifyWebhook(ctx context.Context, headers http.Header, body []byte) (BillingEvent, error)
}

type SandboxProvider struct{}

func (SandboxProvider) CreateCheckout(_ context.Context, request CheckoutRequest) (CheckoutSession, error) {
	return CheckoutSession{
		ID:  "sandbox_" + request.UserID,
		URL: "https://sandbox.example/checkout/" + request.UserID,
	}, nil
}

func (SandboxProvider) VerifyWebhook(_ context.Context, _ http.Header, body []byte) (BillingEvent, error) {
	var event BillingEvent
	if err := json.Unmarshal(body, &event); err != nil {
		return BillingEvent{}, err
	}
	if event.ID == "" || event.UserID == "" || event.Kind == "" {
		return BillingEvent{}, errors.New("invalid billing event")
	}
	return event, nil
}
