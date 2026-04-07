package com.example;

import java.util.HashMap;
import java.util.List;
import java.util.ArrayList;

public class App {
    private HashMap<String, Integer> scores = new HashMap<>();

    public void addScore(String key, int value) {
        scores.put(key, value);
    }

    public List<String> getKeys() {
        return new ArrayList<>(scores.keySet());
    }

    public String describe() {
        StringBuilder sb = new StringBuilder();
        sb.append("Scores: ");
        sb.append(scores.size());
        return sb.toString();
    }
}
