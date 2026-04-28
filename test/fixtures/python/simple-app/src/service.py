"""Service layer."""

from typing import List, Dict, Any

class UserService:
    """User service class."""
    
    def __init__(self):
        self._users: List[Dict[str, Any]] = []
    
    def get_users(self) -> List[Dict[str, Any]]:
        """Get all users."""
        return self._users
    
    def add_user(self, name: str, email: str) -> Dict[str, Any]:
        """Add a new user."""
        user = {"name": name, "email": email}
        self._users.append(user)
        return user
    
    def process(self) -> None:
        """Process all users."""
        for user in self._users:
            self._process_user(user)
    
    def _process_user(self, user: Dict[str, Any]) -> None:
        """Process a single user (private method)."""
        pass
