package com.example;

import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/v2/products")
public class ProductController {

    @GetMapping("/{id}")
    public Product getById(Long id) { return null; }

    @PostMapping("")
    public Product create(Product p) { return null; }

    @GetMapping("")
    public List<Product> list() { return null; }

    @DeleteMapping("/{id}")
    public void delete(Long id) {}
}
