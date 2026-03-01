package auth

import (
	"testing"
	"time"
)

func TestGenerateAndValidateToken(t *testing.T) {
	cfg := Config{
		Secret:  "test-secret",
		Enabled: true,
	}
	mw := New(cfg)

	token, err := mw.GenerateToken("user@example.com", RoleAdmin, 1*time.Hour)
	if err != nil {
		t.Fatalf("GenerateToken failed: %v", err)
	}

	claims, err := mw.ValidateToken(token)
	if err != nil {
		t.Fatalf("ValidateToken failed: %v", err)
	}

	if claims.Sub != "user@example.com" {
		t.Errorf("expected sub 'user@example.com', got '%s'", claims.Sub)
	}
	if claims.Role != RoleAdmin {
		t.Errorf("expected role '%s', got '%s'", RoleAdmin, claims.Role)
	}
}

func TestRevokeToken(t *testing.T) {
	cfg := Config{
		Secret:  "test-secret",
		Enabled: true,
	}
	mw := New(cfg)

	token, _ := mw.GenerateToken("user@example.com", RoleAdmin, 1*time.Hour)
	mw.RevokeToken(token)

	_, err := mw.ValidateToken(token)
	if err == nil {
		t.Fatal("expected error validating revoked token, got nil")
	}
}

func TestHasPermission(t *testing.T) {
	if !hasPermission(RoleAdmin, RoleOperator) {
		t.Error("admin should have operator permission")
	}
	if !hasPermission(RoleOperator, RoleViewer) {
		t.Error("operator should have viewer permission")
	}
	if hasPermission(RoleViewer, RoleAdmin) {
		t.Error("viewer should not have admin permission")
	}
}
