# Basic function
def basic_function():
    print("Basic")

# Function with parameters
def parameterized_function(a: int, b: str = "default") -> str:
    return f"{a} {b}"

# Class definition
class TestClass:
    """Test class docstring"""
    
    def __init__(self):
        self.value = 0
    
    def instance_method(self):
        """Instance method docstring"""
        return self.value
    
    @classmethod
    def class_method(cls):
        return "class method"
    
    @staticmethod
    def static_method():
        return "static"

# Async function
async def async_function():
    return "async"

# Decorator function
def decorator(func):
    def wrapper(*args, **kwargs):
        return func(*args, **kwargs)
    return wrapper

@decorator
def decorated_function():
    pass

# Class with inheritance
class ChildClass(TestClass):
    def child_method(self):
        return super().instance_method()