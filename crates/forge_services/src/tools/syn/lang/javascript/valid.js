// Basic function
function basicFunction() {
    console.log("Basic");
}

// Arrow function
const arrowFunction = () => {
    console.log("Arrow");
};

// Class definition
class TestClass {
    constructor() {
        this.value = 0;
    }

    // Method
    testMethod() {
        return this.value;
    }

    // Static method
    static staticMethod() {
        return "static";
    }
}

// Generator function
function* generatorFunction() {
    yield 1;
    yield 2;
}

// Async function
async function asyncFunction() {
    return Promise.resolve();
}

// Function with documentation
/**
 * Documented function
 * @param {string} input
 * @returns {string}
 */
function documentedFunction(input) {
    return input;
}