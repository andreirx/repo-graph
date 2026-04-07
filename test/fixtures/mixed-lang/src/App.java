package com.example;

import org.springframework.web.bind.annotation.RestController;

@RestController
public class App {
    public String hello() {
        return "hello";
    }
}
