package billing

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
)

type CheckoutService struct {
	provider Provider
	webhooks *WebhookService
}

func NewCheckoutService(provider Provider, webhooks *WebhookService) *CheckoutService {
	return &CheckoutService{provider: provider, webhooks: webhooks}
}

type checkoutRequest struct {
	UserID string `json:"user_id"`
	Plan   string `json:"plan"`
}

type checkoutResponse struct {
	URL       string `json:"url"`
	SessionID string `json:"session_id"`
}

func (s *CheckoutService) CheckoutHandler(writer http.ResponseWriter, request *http.Request) {
	var body checkoutRequest
	if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
		writeError(writer, http.StatusBadRequest, "invalid request")
		return
	}
	session, err := s.provider.CreateCheckout(context.Background(), CheckoutRequest{
		UserID: body.UserID,
		Plan:   body.Plan,
	})
	if err != nil {
		writeError(writer, http.StatusBadGateway, "checkout creation failed")
		return
	}
	writeJSON(writer, http.StatusOK, checkoutResponse{
		URL:       session.URL,
		SessionID: session.ID,
	})
}

func (s *CheckoutService) WebhookHandler(writer http.ResponseWriter, request *http.Request) {
	body, err := io.ReadAll(request.Body)
	if err != nil {
		writeError(writer, http.StatusBadRequest, "invalid webhook body")
		return
	}
	event, err := s.provider.VerifyWebhook(request.Context(), request.Header, body)
	if err != nil {
		writeError(writer, http.StatusBadRequest, "invalid webhook")
		return
	}
	if err := s.webhooks.Handle(event); err != nil {
		writeError(writer, http.StatusInternalServerError, "webhook processing failed")
		return
	}
	writer.WriteHeader(http.StatusNoContent)
}

func writeJSON(writer http.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func writeError(writer http.ResponseWriter, status int, message string) {
	writeJSON(writer, status, map[string]string{"error": message})
}
