"""Application entry point."""

import json
from typing import Optional, List
from .service import UserService

def main() -> int:
    """Main function."""
    service = UserService()
    users = service.get_users()
    return 0

class App:
    """Main application class."""
    
    def __init__(self, name: str):
        self.name = name
        self._service = UserService()
    
    def run(self) -> None:
        """Run the application."""
        self._service.process()

if __name__ == "__main__":
    main()
