package com.example;

import java.sql.DriverManager;
import java.sql.Connection;
import java.sql.SQLException;

/**
 * JE-1 validation corpus: JDBC callsite extraction.
 *
 * This file tests ResolvedCallsite emission for java.sql APIs.
 */
public class App {

    /**
     * Should emit ResolvedCallsite with StringLiteral arg0.
     */
    public void connectLiteral() throws SQLException {
        Connection conn = DriverManager.getConnection("jdbc:h2:mem:testdb");
        // ... use conn
    }

    /**
     * Should emit ResolvedCallsite with StringLiteral arg0.
     * Multiple arguments case.
     */
    public void connectWithCredentials() throws SQLException {
        Connection conn = DriverManager.getConnection(
            "jdbc:postgresql://localhost/mydb",
            "user",
            "password"
        );
    }

    /**
     * Should NOT emit ResolvedCallsite (dynamic arg0).
     */
    public void connectVariable(String url) throws SQLException {
        Connection conn = DriverManager.getConnection(url);
    }

    /**
     * Should NOT emit ResolvedCallsite (arg0 is a method call result).
     */
    public void connectFromConfig() throws SQLException {
        Connection conn = DriverManager.getConnection(getDbUrl());
    }

    private String getDbUrl() {
        return "jdbc:h2:mem:config";
    }
}
