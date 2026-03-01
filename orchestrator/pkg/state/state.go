package state

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
	"time"

	"github.com/redis/go-redis/v9"
)

// StateStore defines the interface for distributed shared state.
// Used for horizontal orchestrator scaling.
type StateStore interface {
	Set(ctx context.Context, key string, value interface{}, expiration time.Duration) error
	Get(ctx context.Context, key string, dest interface{}) error
	Delete(ctx context.Context, key string) error
	Close() error
}

// MemoryState provides in-memory shared state for standalone setups.
type MemoryState struct {
	mu    sync.RWMutex
	store map[string]memoryItem
}

type memoryItem struct {
	data      []byte
	expiresAt time.Time
}

func NewMemoryState() *MemoryState {
	s := &MemoryState{
		store: make(map[string]memoryItem),
	}
	// Start cleanup goroutine
	go s.cleanupLoop()
	return s
}

func (s *MemoryState) Set(ctx context.Context, key string, value interface{}, expiration time.Duration) error {
	data, err := json.Marshal(value)
	if err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Lock()

	expiresAt := time.Time{}
	if expiration > 0 {
		expiresAt = time.Now().Add(expiration)
	}

	s.store[key] = memoryItem{
		data:      data,
		expiresAt: expiresAt,
	}
	return nil
}

func (s *MemoryState) Get(ctx context.Context, key string, dest interface{}) error {
	s.mu.RLock()
	item, ok := s.store[key]
	s.mu.RUnlock()

	if !ok {
		return errors.New("key not found")
	}

	if !item.expiresAt.IsZero() && time.Now().After(item.expiresAt) {
		s.Delete(ctx, key)
		return errors.New("key expired")
	}

	return json.Unmarshal(item.data, dest)
}

func (s *MemoryState) Delete(ctx context.Context, key string) error {
	s.mu.Lock()
	delete(s.store, key)
	s.mu.Unlock()
	return nil
}

func (s *MemoryState) Close() error {
	return nil
}

func (s *MemoryState) cleanupLoop() {
	ticker := time.NewTicker(1 * time.Minute)
	for range ticker.C {
		s.mu.Lock()
		now := time.Now()
		for k, v := range s.store {
			if !v.expiresAt.IsZero() && now.After(v.expiresAt) {
				delete(s.store, k)
			}
		}
		s.mu.Unlock()
	}
}

// RedisState provides Redis-backed shared state for clustered setups.
type RedisState struct {
	client *redis.Client
}

func NewRedisState(redisAddr, password string) *RedisState {
	client := redis.NewClient(&redis.Options{
		Addr:     redisAddr,
		Password: password,
		DB:       0, // use default DB
	})

	return &RedisState{
		client: client,
	}
}

func (s *RedisState) Set(ctx context.Context, key string, value interface{}, expiration time.Duration) error {
	data, err := json.Marshal(value)
	if err != nil {
		return err
	}
	return s.client.Set(ctx, key, data, expiration).Err()
}

func (s *RedisState) Get(ctx context.Context, key string, dest interface{}) error {
	val, err := s.client.Get(ctx, key).Bytes()
	if err != nil {
		return err
	}
	return json.Unmarshal(val, dest)
}

func (s *RedisState) Delete(ctx context.Context, key string) error {
	return s.client.Del(ctx, key).Err()
}

func (s *RedisState) Close() error {
	return s.client.Close()
}
