package com.example

data class Person(val name: String, var age: Int)

class Student(name: String, age: Int, private val grades: List<Double>) : Person(name, age) {
    fun averageGrade(): Double {
        return grades.average()
    }
    
    fun celebrateBirthday() {
        age++
        println("Happy Birthday $name! You are now $age years old.")
    }
}

fun main() {
    val student = Student("Alice", 20, listOf(95.0, 87.5, 92.3))
    student.celebrateBirthday()
    
    val average = student.averageGrade()
    println("Average grade: %.2f".format(average))
    
    // Using collection operations
    val numbers = (1..10).toList()
    val evenSquares = numbers
        .filter { it % 2 == 0 }
        .map { it * it }
    
    println("Even squares: $evenSquares")
}