package resilience

import (
	"errors"
	"testing"
	"time"
)

func TestCircuitBreaker_StateTransitions(t *testing.T) {
	cfg := Config{
		FailureThreshold: 2,
		CooldownDuration: 50 * time.Millisecond,
		SuccessThreshold: 1,
		Name:             "test",
	}
	cb := NewCircuitBreaker(cfg)

	failingFn := func() error { return errors.New("fail") }
	successFn := func() error { return nil }

	// 1st failure - Closed -> Closed
	err := cb.Execute(failingFn)
	if err == nil {
		t.Errorf("expected error, got nil")
	}
	if cb.CurrentState() != StateClosed {
		t.Errorf("expected state %s, got %s", StateClosed, cb.CurrentState())
	}

	// 2nd failure - Closed -> Open
	_ = cb.Execute(failingFn)
	if cb.CurrentState() != StateOpen {
		t.Errorf("expected state %s, got %s", StateOpen, cb.CurrentState())
	}

	// 3rd call immediately - Open -> Error
	err = cb.Execute(successFn)
	if err == nil || err.Error() != "circuit breaker 'test' is open: rejecting call" {
		t.Errorf("expected circuit open error")
	}

	// Wait for cooldown
	time.Sleep(60 * time.Millisecond)

	// Call after cooldown - Open -> HalfOpen -> Closed
	err = cb.Execute(successFn)
	if err != nil {
		t.Errorf("expected success during half-open, got %v", err)
	}

	if cb.CurrentState() != StateClosed {
		t.Errorf("expected state %s after success, got %s", StateClosed, cb.CurrentState())
	}
}
