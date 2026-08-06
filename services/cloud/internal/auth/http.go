package auth

import (
	"encoding/json"
	"net/http"
	"strings"
)

type loginRequest struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

type refreshRequest struct {
	RefreshToken string `json:"refresh_token"`
}

type tokenResponse struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
}

type userResponse struct {
	ID    string `json:"id"`
	Email string `json:"email"`
}

func (s *Service) LoginHandler(writer http.ResponseWriter, request *http.Request) {
	var body loginRequest
	if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
		writeError(writer, http.StatusBadRequest, "invalid request")
		return
	}
	access, refresh, err := s.Login(body.Email, body.Password)
	if err != nil {
		writeError(writer, http.StatusUnauthorized, "invalid credentials")
		return
	}
	writeJSON(writer, http.StatusOK, tokenResponse{
		AccessToken:  access,
		RefreshToken: refresh,
	})
}

func (s *Service) RefreshHandler(writer http.ResponseWriter, request *http.Request) {
	var body refreshRequest
	if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
		writeError(writer, http.StatusBadRequest, "invalid request")
		return
	}
	access, refresh, err := s.Refresh(body.RefreshToken)
	if err != nil {
		writeError(writer, http.StatusUnauthorized, "invalid refresh token")
		return
	}
	writeJSON(writer, http.StatusOK, tokenResponse{
		AccessToken:  access,
		RefreshToken: refresh,
	})
}

func (s *Service) LogoutHandler(writer http.ResponseWriter, request *http.Request) {
	var body refreshRequest
	if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
		writeError(writer, http.StatusBadRequest, "invalid request")
		return
	}
	if err := s.Logout(body.RefreshToken); err != nil {
		writeError(writer, http.StatusUnauthorized, "invalid refresh token")
		return
	}
	writer.WriteHeader(http.StatusNoContent)
}

func (s *Service) MeHandler(writer http.ResponseWriter, request *http.Request) {
	accessToken := strings.TrimPrefix(request.Header.Get("Authorization"), "Bearer ")
	user, err := s.Me(accessToken)
	if err != nil {
		writeError(writer, http.StatusUnauthorized, "invalid access token")
		return
	}
	writeJSON(writer, http.StatusOK, userResponse{ID: user.ID, Email: user.Email})
}

func writeJSON(writer http.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func writeError(writer http.ResponseWriter, status int, message string) {
	writeJSON(writer, status, map[string]string{"error": message})
}
