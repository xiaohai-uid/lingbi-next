package auth

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strings"
	"sync"
	"time"

	"golang.org/x/crypto/bcrypt"
)

type User struct {
	ID           string
	Email        string
	PasswordHash string
}

type Session struct {
	ID        string
	UserID    string
	ExpiresAt time.Time
}

type RefreshTokenRecord struct {
	ID        string
	UserID    string
	TokenHash string
	ExpiresAt time.Time
	Revoked   bool
}

type Service struct {
	mu             sync.RWMutex
	users          map[string]User
	usersByEmail   map[string]string
	sessions       map[string]Session
	refreshTokens  map[string]RefreshTokenRecord
	accessTokenTTL time.Duration
	refreshTTL     time.Duration
}

func NewService() *Service {
	return &Service{
		users:          make(map[string]User),
		usersByEmail:   make(map[string]string),
		sessions:       make(map[string]Session),
		refreshTokens:  make(map[string]RefreshTokenRecord),
		accessTokenTTL: 15 * time.Minute,
		refreshTTL:     30 * 24 * time.Hour,
	}
}

func (s *Service) Register(email, password string) (User, error) {
	email = strings.ToLower(strings.TrimSpace(email))
	if email == "" || len(password) < 8 {
		return User{}, errors.New("invalid email or password")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, exists := s.usersByEmail[email]; exists {
		return User{}, errors.New("email already registered")
	}
	hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return User{}, err
	}
	user := User{
		ID:           randomID("user"),
		Email:        email,
		PasswordHash: string(hash),
	}
	s.users[user.ID] = user
	s.usersByEmail[email] = user.ID
	return user, nil
}

func (s *Service) Login(email, password string) (accessToken, refreshToken string, err error) {
	email = strings.ToLower(strings.TrimSpace(email))
	s.mu.RLock()
	userID, ok := s.usersByEmail[email]
	var user User
	if ok {
		user = s.users[userID]
	}
	s.mu.RUnlock()
	if !ok {
		return "", "", errors.New("invalid credentials")
	}
	if bcrypt.CompareHashAndPassword([]byte(user.PasswordHash), []byte(password)) != nil {
		return "", "", errors.New("invalid credentials")
	}
	return s.createTokens(user.ID)
}

func (s *Service) Refresh(refreshToken string) (accessToken, newRefreshToken string, err error) {
	hash := hashToken(refreshToken)
	s.mu.Lock()
	defer s.mu.Unlock()
	record, ok := s.refreshTokens[hash]
	if !ok || record.Revoked || time.Now().After(record.ExpiresAt) {
		return "", "", errors.New("invalid refresh token")
	}
	record.Revoked = true
	s.refreshTokens[hash] = record
	return s.createTokensLocked(record.UserID)
}

func (s *Service) Logout(refreshToken string) error {
	hash := hashToken(refreshToken)
	s.mu.Lock()
	defer s.mu.Unlock()
	record, ok := s.refreshTokens[hash]
	if !ok {
		return errors.New("invalid refresh token")
	}
	record.Revoked = true
	s.refreshTokens[hash] = record
	return nil
}

func (s *Service) Me(accessToken string) (User, error) {
	s.mu.RLock()
	session, ok := s.sessions[accessToken]
	var user User
	if ok {
		user = s.users[session.UserID]
	}
	s.mu.RUnlock()
	if !ok || time.Now().After(session.ExpiresAt) {
		return User{}, errors.New("invalid access token")
	}
	return user, nil
}

func (s *Service) createTokens(userID string) (string, string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.createTokensLocked(userID)
}

func (s *Service) createTokensLocked(userID string) (string, string, error) {
	accessToken := randomID("access")
	session := Session{
		ID:        accessToken,
		UserID:    userID,
		ExpiresAt: time.Now().Add(s.accessTokenTTL),
	}
	s.sessions[accessToken] = session

	refreshToken := randomID("refresh")
	record := RefreshTokenRecord{
		ID:        randomID("refresh-record"),
		UserID:    userID,
		TokenHash: hashToken(refreshToken),
		ExpiresAt: time.Now().Add(s.refreshTTL),
	}
	s.refreshTokens[record.TokenHash] = record
	return accessToken, refreshToken, nil
}

func hashToken(token string) string {
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:])
}

func randomID(prefix string) string {
	buffer := make([]byte, 32)
	if _, err := rand.Read(buffer); err != nil {
		panic(err)
	}
	return prefix + "_" + hex.EncodeToString(buffer)
}

type EmailSender interface {
	SendVerification(email, code string) error
	SendPasswordReset(email, code string) error
}
