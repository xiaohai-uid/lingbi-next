package entitlement

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"time"
)

type Claims struct {
	Sub                string    `json:"sub"`
	Plan               string    `json:"plan"`
	Features           []string  `json:"features"`
	IssuedAt           time.Time `json:"issued_at"`
	ExpiresAt          time.Time `json:"expires_at"`
	OfflineGraceUntil  time.Time `json:"offline_grace_until"`
	EntitlementVersion int       `json:"entitlement_version"`
}

type Service struct {
	privateKey ed25519.PrivateKey
}

func New(privateKey ed25519.PrivateKey) *Service {
	return &Service{privateKey: privateKey}
}

func (s *Service) Issue(sub, plan string, features []string) (string, error) {
	now := time.Now().UTC()
	claims := Claims{
		Sub:                sub,
		Plan:               plan,
		Features:           features,
		IssuedAt:           now,
		ExpiresAt:          now.Add(30 * 24 * time.Hour),
		OfflineGraceUntil:  now.Add(7 * 24 * time.Hour),
		EntitlementVersion: 1,
	}
	payload, err := json.Marshal(claims)
	if err != nil {
		return "", err
	}
	signature := ed25519.Sign(s.privateKey, payload)
	return base64.RawURLEncoding.EncodeToString(payload) + "." +
		base64.RawURLEncoding.EncodeToString(signature), nil
}

func (s *Service) Verify(token string) (Claims, error) {
	parts := strings.Split(token, ".")
	if len(parts) != 2 {
		return Claims{}, errors.New("invalid entitlement token")
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		return Claims{}, errors.New("invalid entitlement payload")
	}
	signature, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return Claims{}, errors.New("invalid entitlement signature")
	}
	publicKey := s.privateKey.Public().(ed25519.PublicKey)
	if !ed25519.Verify(publicKey, payload, signature) {
		return Claims{}, errors.New("invalid entitlement signature")
	}
	var claims Claims
	if err := json.Unmarshal(payload, &claims); err != nil {
		return Claims{}, errors.New("invalid entitlement claims")
	}
	if time.Now().After(claims.ExpiresAt) {
		return Claims{}, errors.New("entitlement expired")
	}
	return claims, nil
}

func (s *Service) PublicKey() ed25519.PublicKey {
	return s.privateKey.Public().(ed25519.PublicKey)
}
