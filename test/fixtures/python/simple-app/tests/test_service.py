"""Tests for service module."""

from src.service import UserService

class TestUserService:
    """Test cases for UserService."""
    
    def test_get_users_empty(self):
        """Test getting users from empty service."""
        service = UserService()
        assert service.get_users() == []
    
    def test_add_user(self):
        """Test adding a user."""
        service = UserService()
        user = service.add_user("Alice", "alice@example.com")
        assert user["name"] == "Alice"
