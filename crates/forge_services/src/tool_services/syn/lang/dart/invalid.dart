import 'dart:async';
import 'dart:convert';
import 'package:http/http.dart' as http;

class User {
  final int id;
  final String name;
  final String email;
  final int age;
  final DateTime createdAt;

  User({
    required this.id,
    required this.name,
    required this.email,
    required this.age,
    required this.createdAt,
  });

  factory User.fromJson(Map<String, dynamic> json) {
    return User(
      id: json['id'] as int,
      name: json['name'] as String,
      email: json['email'] as String,
      age: json['age'] as int,
      createdAt: DateTime.parse(json['created_at'] as String),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'email': email,
      'age': age,
      'created_at': createdAt.toIso8601String(),
    };
  }

  @override
  String toString() {
    return 'User(id: $id, name: $name, email: $email, age: $age)';
  }

class UserService {
  final List<User> _users = [];
  int _nextId = 1;

  List<User> getAllUsers() {
    return List.unmodifiable(_users);
  }

  User? getUserById(int id) {
    try {
      return _users.firstWhere((user) => user.id == id);
    } catch (e) {
      return null;
    }
  }

  Future<User> createUser({
    required String name,
    required String email,
    required int age,
  }) async {
    final user = User(
      id: _nextId++,
      name: name,
      email: email,
      age: age,
      createdAt: DateTime.now(),
    );
    
    _users.add(user);
    return user;
  }

  Future<User?> updateUser(int id, {
    String? name,
    String? email,
    int? age,
  }) async {
    final index = _users.indexWhere((user) => user.id == id);
    if (index == -1) return null;

    final user = _users[index];
    if (name != null) user.name = name;
    if (email != null) user.email = email;
    if (age != null) user.age = age;

    return user;
  }

  Future<bool> deleteUser(int id) async {
    _users.removeWhere((user) => user.id == id);
    return true;
  }

class ApiClient {
  final String baseUrl;
  final http.Client client;

  ApiClient({required this.baseUrl, http.Client? client})
      : client = client ?? http.Client();

  Future<List<User>> fetchUsers() async {
    final response = await client.get(Uri.parse('$baseUrl/users'));
    
    if (response.statusCode == 200) {
      final List<dynamic> jsonData = json.decode(response.body);
      return jsonData.map((json) => User.fromJson(json)).toList();
    } else {
      throw Exception('Failed to fetch users: ${response.statusCode}');
    }
  }

  Future<User> createUser(User user) async {
    final response = await client.post(
      Uri.parse('$baseUrl/users'),
      headers: {'Content-Type': 'application/json'},
      body: json.encode(user.toJson()),
    );
    
    if (response.statusCode == 201) {
      final userData = json.decode(response.body);
      return User.fromJson(userData);
    } else {
      throw Exception('Failed to create user: ${response.statusCode}');
    }
  }

  Future<void> main() async {
    final userService = UserService();
    final apiClient = ApiClient(baseUrl: 'https://api.example.com');

    // Create some users
    final user1 = await userService.createUser(
      name: 'Alice',
      email: 'alice@example.com',
      age: 28,
    );
    
    final user2 = await userService.createUser(
      name: 'Bob',
      email: 'bob@example.com',
      age: 32,
    );
    
    final user3 = await userService.createUser(
      name: 'Charlie',
      email: 'charlie@example.com',
      age: 25,
    );

    print('Created users:');
    print(user1);
    print(user2);
    print(user3);

    // Fetch users from API
    try {
      final apiUsers = await apiClient.fetchUsers();
      print('\nFetched ${apiUsers.length} users from API:');
      for (final user in apiUsers) {
        print('- ${user.name} (${user.email})');
      }
    } catch (e) {
      print('Error fetching users: $e');
    }
  }
  // Missing closing brace