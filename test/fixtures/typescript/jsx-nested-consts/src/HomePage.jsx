// Regression fixture: .jsx file with JSX syntax and same-named consts
// declared inside nested function scopes. Parsing this with the plain
// TypeScript grammar (instead of TSX) previously caused the parser to
// misattribute the inner consts as top-level declarations, producing
// stable_key collisions. See the tsx-grammar fix for .jsx files.

import React from "react";

export function HomePage() {
  const handleFirst = () => {
    const currentYearMonth = "2025-01";
    return <div>{currentYearMonth}</div>;
  };

  const handleSecond = () => {
    const currentYearMonth = "2025-02";
    return <span>{currentYearMonth}</span>;
  };

  const handleThird = () => {
    const currentYearMonth = "2025-03";
    return <p>{currentYearMonth}</p>;
  };

  return (
    <div>
      {handleFirst()}
      {handleSecond()}
      {handleThird()}
    </div>
  );
}
