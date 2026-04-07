import os
import requests
from typing import List, Optional
from .utils import helper


class UserService:
    def __init__(self, db):
        self.db = db

    def get_user(self, user_id: int) -> Optional[dict]:
        return self.db.find(user_id)


def process_items(items: List[str]) -> int:
    count = 0
    for item in items:
        count += 1
    return count


API_URL = "https://api.example.com"
