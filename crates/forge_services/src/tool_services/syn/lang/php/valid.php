<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;
use Illuminate\Http\Response;

class UserController
{
    private array $users = [];
    
    public function __construct()
    {
        $this->users = [
            ['id' => 1, 'name' => 'Alice', 'email' => 'alice@example.com'],
            ['id' => 2, 'name' => 'Bob', 'email' => 'bob@example.com'],
            ['id' => 3, 'name' => 'Charlie', 'email' => 'charlie@example.com'],
        ];
    }
    
    public function index(): Response
    {
        return response()->json($this->users);
    }
    
    public function show(int $id): Response
    {
        $user = collect($this->users)->firstWhere('id', $id);
        
        if (!$user) {
            return response()->json(['error' => 'User not found'], 404);
        }
        
        return response()->json($user);
    }
    
    public function store(Request $request): Response
    {
        $validated = $request->validate([
            'name' => 'required|string|max:255',
            'email' => 'required|email|unique:users',
            'age' => 'required|integer|min:18'
        ]);
        
        $user = [
            'id' => count($this->users) + 1,
            'name' => $validated['name'],
            'email' => $validated['email'],
            'age' => $validated['age'],
            'created_at' => date('Y-m-d H:i:s')
        ];
        
        $this->users[] = $user;
        
        return response()->json($user, 201);
    }
    
    public function update(int $id, Request $request): Response
    {
        $userIndex = collect($this->users)->search(fn($user) => $user['id'] === $id);
        
        if ($userIndex === false) {
            return response()->json(['error' => 'User not found'], 404);
        }
        
        $validated = $request->validate([
            'name' => 'sometimes|string|max:255',
            'email' => 'sometimes|email',
            'age' => 'sometimes|integer|min:18'
        ]);
        
        $this->users[$userIndex] = array_merge(
            $this->users[$userIndex],
            array_filter($validated, fn($value) => $value !== null)
        );
        
        return response()->json($this->users[$userIndex]);
    }
    
    public function destroy(int $id): Response
    {
        $userIndex = collect($this->users)->search(fn($user) => $user['id'] === $id);
        
        if ($userIndex === false) {
            return response()->json(['error' => 'User not found'], 404);
        }
        
        unset($this->users[$userIndex]);
        
        return response()->json(null, 204);
    }
    
    private function calculateAge(array $user): int
    {
        $birthDate = new \DateTime($user['birth_date'] ?? '2000-01-01');
        $currentDate = new \DateTime();
        
        return $birthDate->diff($currentDate)->y;
    }
}