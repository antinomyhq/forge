import Foundation

class Person {
    let name: String
    var age: Int
    
    init(name: String, age: Int) {
        self.name = name
        self.age = age
    }
    
    func celebrateBirthday() {
        age += 1
        print("Happy Birthday \(name)! You are now \(age) years old.")
    }
}

class Student: Person {
    var grades: [Double]
    
    init(name: String, age: Int, grades: [Double]) {
        self.grades = grades
        super.init(name: name, age: age)
    }
    
    func averageGrade() -> Double {
        return grades.reduce(0, +) / Double(grades.count)
    }
}

let student = Student(name: "Alice", age: 20, grades: [95.0, 87.5, 92.3])
student.celebrateBirthday()
print("Average grade: \(student.averageGrade()")

// Missing closing parenthesis