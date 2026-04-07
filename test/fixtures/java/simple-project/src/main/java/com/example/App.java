package com.example;

import java.util.HashMap;
import java.util.List;
import java.util.ArrayList;

public class App {
    private String name;
    private HashMap<String, Integer> scores;

    public App(String name) {
        this.name = name;
        this.scores = new HashMap<>();
    }

    public App() {
        this("default");
    }

    public void addScore(String key, int value) {
        scores.put(key, value);
    }

    public void addScore(String key) {
        addScore(key, 0);
    }

    public List<String> getKeys() {
        return new ArrayList<>(scores.keySet());
    }

    // Same-arity overloads: both take 1 param, different types.
    // Must produce distinct stable keys.
    public String format(String input) {
        return input.trim();
    }

    public String format(Integer number) {
        return number.toString();
    }

    private void helper() {
        System.out.println(name);
    }
}
