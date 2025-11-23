#include <stdio.h>
#include <stdlib.h>

typedef struct {
    char* name;
    int age;
    double score;
} Student;

int main() {
    Student students[3];
    
    // Initialize students
    students[0] = (Student){"Alice", 20, 95.5};
    students[1] = (Student){"Bob", 21, 87.0};
    students[2] = (Student){"Charlie", 19, 92.3};
    
    printf("Student List:\n");
    for (int i = 0; i < 3; i++) {
        printf("%s: Age %d, Score %.1f\n", 
               students[i].name, students[i].age, students[i].score);
    }
    
    // Missing return statement
    missing_semicolon_here
    
    // Missing closing brace for main